mod app;
mod painter;
mod theme;

#[cfg(target_os = "macos")]
mod macos_display;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_winit::winit;
use glutin::context::NotCurrentGlContext;
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::prelude::GlSurface;
use raw_window_handle::HasWindowHandle;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::Window;

use navi_core::{
    config::{detect_db, expand_tilde, Config},
    load_graph, Graph,
};

/// Cross-thread events delivered to winit's main event loop.
///
/// `Vsync` is the heart of the new pacer: a dedicated background thread runs a
/// `CADisplayLink` against its own NSRunLoop, and on each tick (4.17 ms on this
/// 240 Hz panel) it sends one of these to the main thread. We render exactly
/// once per vsync — no mouse-driven extra repaints, no main-thread sleeps.
#[derive(Debug)]
pub enum UserEvent {
    Vsync,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cfg = Config::load();
    if cfg.db.is_empty() {
        cfg.db = detect_db();
    }
    if cfg.emacsclient.is_empty() {
        cfg.emacsclient = navi_core::EmacsClient::new("", &cfg.server_name)
            .binary
            .clone();
    }
    let db_path = expand_tilde(&cfg.db);

    let (raw_nodes, raw_edges) = load_graph(&db_path).map_err(|e| {
        eprintln!("navi: failed to load {db_path}: {e}");
        e
    })?;
    let n_nodes = raw_nodes.len();
    let n_edges = raw_edges.len();
    let graph = Graph::new(raw_nodes, raw_edges);
    eprintln!("navi: loaded {n_nodes} nodes, {n_edges} edges");
    cfg.save();

    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();

    let mut app_state = AppState::new(graph, cfg, db_path, proxy);
    event_loop.run_app(&mut app_state)?;
    Ok(())
}

/// All runtime state owned by the winit `ApplicationHandler`.
struct AppState {
    proxy: EventLoopProxy<UserEvent>,
    graph: Option<Graph>,
    cfg: Option<Config>,
    db_path: Option<String>,

    // Created on `resumed` once we have an `ActiveEventLoop`.
    gl_window: Option<GlutinWindow>,
    gl: Option<Arc<glow::Context>>,
    egui_glow: Option<egui_glow::EguiGlow>,
    app: Option<app::NaviApp>,

    // Idle scheduling: when we're not active, we don't run the display link;
    // we wake on a timer instead. `next_idle_wake` tells `new_events` that the
    // resume-time fired and a redraw is due.
    next_idle_wake: Option<Instant>,

    // Stats
    frame_times: std::collections::VecDeque<f32>,
    last_frame: Instant,
    last_fps_log: Option<Instant>,

    // Wall-clock of the last actual paint. Used to coalesce back-to-back
    // redraw triggers (e.g. a Vsync user-event arriving microseconds after a
    // RedrawRequested from a focus/input event). At 240 Hz the vsync window is
    // ~4.17 ms; anything closer than half that is duplicate work that just
    // contends with the compositor for the same presentation slot.
    last_paint_at: Option<Instant>,
}

impl AppState {
    fn new(graph: Graph, cfg: Config, db_path: String, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            proxy,
            graph: Some(graph),
            cfg: Some(cfg),
            db_path: Some(db_path),
            gl_window: None,
            gl: None,
            egui_glow: None,
            app: None,
            next_idle_wake: None,
            frame_times: std::collections::VecDeque::new(),
            last_frame: Instant::now(),
            last_fps_log: None,
            last_paint_at: None,
        }
    }
}

impl ApplicationHandler<UserEvent> for AppState {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gl_window.is_some() {
            return;
        }

        let (gl_window, gl) = unsafe { GlutinWindow::create(event_loop) };
        let gl = Arc::new(gl);
        gl_window.window().set_visible(true);

        let egui_glow = egui_glow::EguiGlow::new(event_loop, gl.clone(), None, None, true);
        setup_fonts(&egui_glow.egui_ctx);

        // Spawn the display link on a dedicated bg thread. From there it sends
        // `UserEvent::Vsync` to the main thread on every tick. The link is
        // started in the *paused* state — we'll resume it whenever the app
        // enters its full-speed mode.
        #[cfg(target_os = "macos")]
        macos_display::install_bg_link(gl_window.window(), self.proxy.clone());

        let graph = self.graph.take().expect("graph initialised in main()");
        let cfg = self.cfg.take().expect("cfg initialised in main()");
        let db_path = self.db_path.take().expect("db_path initialised in main()");
        let app = app::NaviApp::new(graph, cfg, db_path);

        self.gl_window = Some(gl_window);
        self.gl = Some(gl);
        self.egui_glow = Some(egui_glow);
        self.app = Some(app);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
            event_loop.exit();
            return;
        }
        if let WindowEvent::Resized(size) = &event {
            if let Some(gw) = self.gl_window.as_ref() {
                gw.resize(*size);
            }
        }

        if matches!(event, WindowEvent::RedrawRequested) {
            self.redraw(event_loop);
            return;
        }

        // True iff we're currently in idle (link paused, waiting on the 33 ms
        // resume timer). In that mode we *do* want input/focus events to kick
        // an immediate redraw so the cadence transition happens within one
        // frame. In active mode we explicitly do NOT request_redraw — the
        // CADisplayLink Vsync user-events are the sole paint trigger, and any
        // extra request_redraw causes a double-paint that contends with the
        // compositor and produces a missed-vsync outlier.
        let is_idle = self.next_idle_wake.is_some();

        // Focus changes drive the idle/active cadence. Gaining focus instantly
        // restores the full-speed tier; losing focus lets the next paint drop
        // us straight into idle mode.
        if let WindowEvent::Focused(focused) = &event {
            if let Some(app) = self.app.as_mut() {
                app.set_focused(*focused);
            }
            if *focused && is_idle {
                if let Some(gw) = self.gl_window.as_ref() {
                    gw.window().request_redraw();
                }
            }
        }

        // Any user input bumps the activity timer (keeps us in full-speed mode
        // for `idle_grace`). Only kick a redraw if we're idle right now.
        let is_input = matches!(
            event,
            WindowEvent::CursorMoved { .. }
                | WindowEvent::CursorEntered { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::KeyboardInput { .. }
                | WindowEvent::ModifiersChanged(..)
                | WindowEvent::Ime(..)
                | WindowEvent::Touch(..)
                | WindowEvent::PinchGesture { .. }
                | WindowEvent::PanGesture { .. }
                | WindowEvent::RotationGesture { .. }
        );
        if is_input {
            if let Some(app) = self.app.as_mut() {
                app.touch_input();
            }
            if is_idle {
                if let Some(gw) = self.gl_window.as_ref() {
                    gw.window().request_redraw();
                }
            }
        }

        // Feed the event to egui_winit so it can update its input state. We
        // *deliberately* ignore `event_response.repaint` — the whole point of
        // this rewrite is to decouple our redraw cadence from input arrival.
        // Display vsync ticks are the only signal that triggers a paint.
        if let (Some(eg), Some(gw)) = (self.egui_glow.as_mut(), self.gl_window.as_ref()) {
            let _ = eg.on_window_event(gw.window(), &event);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Vsync => {
                // Paint *directly* — skip the request_redraw → RedrawRequested
                // round-trip through winit's queue. That extra hop costs us
                // some latency on every frame and, when we're close to the
                // vsync deadline, occasionally pushes us into the next vsync
                // window and produces a 8–12 ms outlier (visible micro-stutter).
                self.redraw(event_loop);
            }
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        if let winit::event::StartCause::ResumeTimeReached { .. } = cause {
            if let Some(gw) = self.gl_window.as_ref() {
                gw.window().request_redraw();
            }
            self.next_idle_wake = None;
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(eg) = self.egui_glow.as_mut() {
            eg.destroy();
        }
    }
}

impl AppState {
    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();

        // Coalesce duplicate redraw triggers. At 240 Hz the vsync window is
        // ~4.17 ms; a paint that lands within half of that of the previous
        // paint is a duplicate (e.g. a Vsync arrived microseconds after a
        // RedrawRequested from the same vsync, or two redraw paths fired in
        // rapid succession). Painting twice in the same compositor window
        // contends for the same presentation slot and shows up as an 8–12 ms
        // worst-case frame in the FPS log even when avg is locked at 4.17.
        if let Some(prev) = self.last_paint_at {
            if now.duration_since(prev) < Duration::from_micros(2_000) {
                return;
            }
        }

        // Frame timing
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        self.frame_times.push_back(dt);
        while self.frame_times.iter().sum::<f32>() > 1.0 && self.frame_times.len() > 2 {
            self.frame_times.pop_front();
        }

        let needs_full_speed = match self.app.as_ref() {
            Some(a) => a.needs_full_speed(),
            None => false,
        };

        // Toggle the display link based on app state. Active = link runs at
        // 240 Hz and pumps Vsync events; Idle = link paused, OS drops the
        // refresh tier, we use a 33 ms timer for the next wake-up.
        #[cfg(target_os = "macos")]
        macos_display::set_active(needs_full_speed);
        if !needs_full_speed {
            let next = now + Duration::from_millis(33);
            self.next_idle_wake = Some(next);
            event_loop.set_control_flow(ControlFlow::WaitUntil(next));
        } else {
            // While active, control flow stays Wait — we paint only when a
            // Vsync user-event arrives. No polling, no busy-looping. Clear
            // `next_idle_wake` so the input/focus handlers in window_event
            // know we're no longer in the idle path and don't fire extra
            // request_redraw calls (which would cause double-paints).
            self.next_idle_wake = None;
            event_loop.set_control_flow(ControlFlow::Wait);
        }

        // Run the egui frame.
        let gw = self.gl_window.as_mut().expect("gl_window");
        let eg = self.egui_glow.as_mut().expect("egui_glow");
        let app = self.app.as_mut().expect("app");

        eg.run(gw.window(), |ctx| {
            app.update(ctx);
        });

        // Paint.
        let theme_bg = app.bg_color();
        unsafe {
            use glow::HasContext as _;
            let gl = self.gl.as_ref().expect("gl");
            gl.clear_color(theme_bg[0], theme_bg[1], theme_bg[2], 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        eg.paint(gw.window());
        let _ = gw.swap_buffers();
        self.last_paint_at = Some(now);

        // Diagnostics
        if std::env::var_os("NAVI_FPS_LOG").is_some() {
            let due = self
                .last_fps_log
                .map_or(true, |t| t.elapsed() >= Duration::from_secs(1));
            if due && self.frame_times.len() >= 2 {
                let n = self.frame_times.len() as f32;
                let total: f32 = self.frame_times.iter().sum();
                let avg_ms = total / n * 1000.0;
                let max_ms = self
                    .frame_times
                    .iter()
                    .copied()
                    .fold(0.0_f32, f32::max)
                    * 1000.0;
                let fps = n / total;
                #[cfg(target_os = "macos")]
                let link_info = {
                    let ticks = macos_display::tick_count();
                    let interval_us = macos_display::vsync_interval()
                        .map(|d| d.as_micros() as u64)
                        .unwrap_or(0);
                    let hz = if interval_us > 0 {
                        1_000_000.0 / interval_us as f64
                    } else {
                        0.0
                    };
                    format!("  link_total_ticks={ticks}  link_interval={interval_us}us ({hz:.0}Hz)")
                };
                #[cfg(not(target_os = "macos"))]
                let link_info = String::new();
                eprintln!(
                    "navi: fps={:.1}  avg={:.2}ms  worst={:.2}ms  frames={}{}",
                    fps,
                    avg_ms,
                    max_ms,
                    self.frame_times.len(),
                    link_info,
                );
                self.last_fps_log = Some(now);
            }
        }
    }
}

// ─── Glutin context plumbing ───────────────────────────────────────────────────

struct GlutinWindow {
    window: Window,
    gl_context: glutin::context::PossiblyCurrentContext,
    gl_display: glutin::display::Display,
    gl_surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
}

impl GlutinWindow {
    /// Construct the window + GL context. Adapted from egui_glow's pure_glow example.
    /// Vsync is left ON (`SwapInterval::Wait(1)`) — it's harmless at 0.5 ms render
    /// time and gives the OS a clean signal that we want display sync.
    unsafe fn create(event_loop: &ActiveEventLoop) -> (Self, glow::Context) {
        let window_attrs = winit::window::WindowAttributes::default()
            .with_resizable(true)
            .with_inner_size(winit::dpi::LogicalSize::new(1400.0, 900.0))
            .with_min_inner_size(winit::dpi::LogicalSize::new(400.0, 300.0))
            .with_title("Navi")
            .with_visible(false);

        let cfg_template = glutin::config::ConfigTemplateBuilder::new()
            .prefer_hardware_accelerated(None)
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(false);

        let (mut window_opt, gl_config) = glutin_winit::DisplayBuilder::new()
            .with_preference(glutin_winit::ApiPreference::FallbackEgl)
            .with_window_attributes(Some(window_attrs.clone()))
            .build(event_loop, cfg_template, |mut it| {
                it.next().expect("no matching gl config")
            })
            .expect("failed to create gl_config");

        let gl_display = gl_config.display();
        let raw_window_handle = window_opt
            .as_ref()
            .map(|w| w.window_handle().expect("window handle").as_raw());

        let context_attrs =
            glutin::context::ContextAttributesBuilder::new().build(raw_window_handle);
        let fallback_attrs = glutin::context::ContextAttributesBuilder::new()
            .with_context_api(glutin::context::ContextApi::Gles(None))
            .build(raw_window_handle);

        let not_current = unsafe {
            gl_display
                .create_context(&gl_config, &context_attrs)
                .unwrap_or_else(|_| {
                    gl_display
                        .create_context(&gl_config, &fallback_attrs)
                        .expect("failed to create gl context")
                })
        };

        let window = window_opt.take().unwrap_or_else(|| {
            glutin_winit::finalize_window(event_loop, window_attrs.clone(), &gl_config)
                .expect("failed to finalize window")
        });

        let (w, h): (u32, u32) = window.inner_size().into();
        let surface_attrs =
            glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
                .build(
                    window.window_handle().expect("window handle").as_raw(),
                    NonZeroU32::new(w).unwrap_or(NonZeroU32::MIN),
                    NonZeroU32::new(h).unwrap_or(NonZeroU32::MIN),
                );

        let gl_surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attrs)
                .expect("failed to create surface")
        };
        let gl_context = not_current
            .make_current(&gl_surface)
            .expect("failed to make context current");

        // Vsync OFF. macOS OpenGL's swap-vsync is capped at ~108 Hz on ProMotion
        // panels regardless of what tier the display is actually running at, so
        // letting it block here would clamp us to 108 fps even though the display
        // link is firing at 240 Hz. Pacing is owned entirely by the bg-thread
        // display link in macos_display.rs.
        let _ = gl_surface.set_swap_interval(&gl_context, glutin::surface::SwapInterval::DontWait);

        let gl = unsafe {
            glow::Context::from_loader_function(|s| {
                let cstr = std::ffi::CString::new(s).unwrap();
                gl_display.get_proc_address(&cstr)
            })
        };

        (
            Self {
                window,
                gl_context,
                gl_display,
                gl_surface,
            },
            gl,
        )
    }

    fn window(&self) -> &Window {
        &self.window
    }

    fn resize(&self, size: winit::dpi::PhysicalSize<u32>) {
        let _ = &self.gl_display; // silence "unused"
        self.gl_surface.resize(
            &self.gl_context,
            NonZeroU32::new(size.width).unwrap_or(NonZeroU32::MIN),
            NonZeroU32::new(size.height).unwrap_or(NonZeroU32::MIN),
        );
    }

    fn swap_buffers(&self) -> glutin::error::Result<()> {
        self.gl_surface.swap_buffers(&self.gl_context)
    }
}

// ─── Fonts ────────────────────────────────────────────────────────────────────

/// Load a prioritised font fallback chain so egui can render:
///   • Nerd Font symbols (powerline, devicons, file icons)  — Meslo NF
///   • Japanese / CJK                                       — Noto Sans JP
///   • Broad Unicode catch-all (~50 k glyphs)               — Arial Unicode
///   • Extra maths / misc symbols                           — Noto Sans Symbols 2
fn setup_fonts(ctx: &egui::Context) {
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates: &[(&str, String)] = &[
        ("nerd",     format!("{home}/Library/Fonts/MesloLGLNerdFont-Regular.ttf")),
        ("jp",       "/Library/Fonts/NotoSansJP-Regular.otf".into()),
        ("unicode",  "/Library/Fonts/Arial Unicode.ttf".into()),
        ("symbols2", "/Library/Fonts/NotoSansSymbols2-Regular.ttf".into()),
    ];
    let mut fonts = egui::FontDefinitions::default();
    for (name, path) in candidates {
        if let Ok(data) = std::fs::read(path) {
            fonts
                .font_data
                .insert((*name).to_string(), egui::FontData::from_owned(data));
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push((*name).to_string());
            }
        }
    }
    ctx.set_fonts(fonts);
}
