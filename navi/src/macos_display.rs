//! macOS display tier control + vsync source.
//!
//! Architecture:
//!   * A dedicated background thread owns a `CADisplayLink` attached to its own
//!     `NSRunLoop`. The main thread cannot starve it (it has no work running on
//!     the bg thread other than the link callback).
//!   * On each vsync the callback sends `UserEvent::Vsync` to winit's main
//!     event loop via `EventLoopProxy`. main.rs treats that as "please paint".
//!   * `set_active(true)` resumes the link; `set_active(false)` pauses it (the
//!     OS may drop the panel back to a lower refresh tier when paused, saving
//!     power). The bg thread itself stays alive for the process lifetime.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, AllocAnyThread};
use objc2_app_kit::NSScreen;
use objc2_foundation::{
    MainThreadMarker, NSDate, NSObject, NSObjectProtocol, NSRunLoop, NSRunLoopMode,
};
use objc2_quartz_core::{CADisplayLink, CAFrameRateRange};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::event_loop::EventLoopProxy;

use crate::UserEvent;

// ─── Atomics for diagnostics + pacer queries ──────────────────────────────────

static ORIGIN: OnceLock<std::time::Instant> = OnceLock::new();
static LAST_VSYNC_NS: AtomicU64 = AtomicU64::new(0);
static VSYNC_INTERVAL_US: AtomicU64 = AtomicU64::new(0);
static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// Pause-state for the link. `true` = run; `false` = paused.
static LINK_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// ─── Cross-thread plumbing ────────────────────────────────────────────────────

struct ProxyHandle(EventLoopProxy<UserEvent>);
// SAFETY: `EventLoopProxy<UserEvent>` is `Send` and we only read+clone on the
// bg thread, never share a `&` reference across threads.
unsafe impl Send for ProxyHandle {}
unsafe impl Sync for ProxyHandle {}

/// Holds the proxy that the link's tick callback uses to wake the main loop.
static EVENT_PROXY: Mutex<Option<ProxyHandle>> = Mutex::new(None);

/// Holds a pointer to the running link so `set_active` can call `setPaused:`.
/// Stored as raw `*mut CADisplayLink` because `Retained` isn't `Send` and the
/// link lives forever on the bg thread anyway.
struct LinkPtr(*mut CADisplayLink);
// SAFETY: we only use the pointer to invoke `setPaused:` which is documented
// to be safe to call from any thread.
unsafe impl Send for LinkPtr {}
unsafe impl Sync for LinkPtr {}
static LINK: OnceLock<LinkPtr> = OnceLock::new();

// ─── Objective-C delegate target ──────────────────────────────────────────────

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "NaviDisplayLinkTarget"]
    #[derive(Debug)]
    struct LinkTarget;

    unsafe impl NSObjectProtocol for LinkTarget {}

    impl LinkTarget {
        #[unsafe(method(tick:))]
        fn tick(&self, link: &CADisplayLink) {
            let now = std::time::Instant::now();
            let since = now.duration_since(*ORIGIN.get_or_init(std::time::Instant::now));
            LAST_VSYNC_NS.store(since.as_nanos() as u64, Ordering::Relaxed);
            TICK_COUNT.fetch_add(1, Ordering::Relaxed);

            unsafe {
                let dur: f64 = msg_send![link, duration];
                if dur > 0.0 && dur.is_finite() {
                    VSYNC_INTERVAL_US.store((dur * 1_000_000.0) as u64, Ordering::Relaxed);
                }
            }

            // Wake the main thread — this is what drives the actual paint cadence.
            if let Ok(g) = EVENT_PROXY.lock() {
                if let Some(p) = g.as_ref() {
                    let _ = p.0.send_event(UserEvent::Vsync);
                }
            }
        }
    }
);

// ─── Public API ───────────────────────────────────────────────────────────────

/// Spawn the dedicated bg thread, create a `CADisplayLink` pinned to the panel's
/// max refresh rate, and start it paused. Call once at startup, after the window
/// has been created (we need the NSScreen the window is on).
pub fn install_bg_link(window: &winit::window::Window, proxy: EventLoopProxy<UserEvent>) {
    let _ = ORIGIN.get_or_init(std::time::Instant::now);

    // Stash the proxy where the bg thread's link callback can reach it.
    if let Ok(mut g) = EVENT_PROXY.lock() {
        *g = Some(ProxyHandle(proxy));
    }

    // Determine the panel's max refresh rate. NSScreen is not strictly
    // main-thread-only, but `screen()` on an NSWindow is — fetch it now.
    let max_fps: f32 = match max_fps_from_window(window) {
        Some(f) => {
            eprintln!("navi: display maximumFramesPerSecond = {}", f as u32);
            f
        }
        None => {
            eprintln!("navi: could not query NSScreen.maximumFramesPerSecond — defaulting to 240 Hz");
            240.0
        }
    };

    std::thread::Builder::new()
        .name("navi-display-link".into())
        .spawn(move || run_bg_link(max_fps))
        .expect("spawn navi-display-link thread");
}

/// Pause / unpause the link. Safe to call from the main thread (or anywhere).
pub fn set_active(active: bool) {
    LINK_ACTIVE.store(active, Ordering::Relaxed);
    if let Some(ptr) = LINK.get() {
        // SAFETY: `setPaused:` is safe to call on `CADisplayLink` from any thread.
        unsafe {
            let link: &CADisplayLink = &*ptr.0;
            link.setPaused(!active);
        }
    }
}

/// Most recent display vsync timestamp. Mostly for diagnostics.
pub fn _last_vsync() -> Option<std::time::Instant> {
    let ns = LAST_VSYNC_NS.load(Ordering::Relaxed);
    if ns == 0 {
        return None;
    }
    let origin = *ORIGIN.get()?;
    Some(origin + std::time::Duration::from_nanos(ns))
}

/// Last reported display vsync interval (e.g. 4166 µs at 240 Hz).
pub fn vsync_interval() -> Option<std::time::Duration> {
    let us = VSYNC_INTERVAL_US.load(Ordering::Relaxed);
    if us == 0 {
        None
    } else {
        Some(std::time::Duration::from_micros(us))
    }
}

pub fn tick_count() -> u64 {
    TICK_COUNT.load(Ordering::Relaxed)
}

// ─── Internals ────────────────────────────────────────────────────────────────

/// Body of the bg thread. Sets up the link, adds it to this thread's run loop,
/// then runs the loop forever.
fn run_bg_link(max_fps: f32) {
    let target: Retained<LinkTarget> = unsafe {
        let alloc = LinkTarget::alloc();
        msg_send![alloc, init]
    };
    let target_obj: &AnyObject =
        unsafe { &*((&*target) as *const LinkTarget as *const AnyObject) };

    // We can construct the link from any NSScreen instance. NSScreen.mainScreen()
    // works fine on a non-main thread for read access on modern macOS.
    let link: Retained<CADisplayLink> = unsafe {
        let screen_opt = bg_screen();
        let screen: &NSScreen = match screen_opt.as_deref() {
            Some(s) => s,
            None => {
                eprintln!("navi: bg-thread display link could not find an NSScreen — pacer disabled");
                return;
            }
        };
        screen.displayLinkWithTarget_selector(target_obj, sel!(tick:))
    };

    // Pin to the panel's max refresh tier (240 Hz on the user's display).
    let range = CAFrameRateRange::new(max_fps, max_fps, max_fps);
    link.setPreferredFrameRateRange(range);

    // Start paused. main.rs will call set_active(true) when interaction begins.
    link.setPaused(true);

    // Install on this thread's run loop.
    unsafe {
        let runloop = NSRunLoop::currentRunLoop();
        extern "C" {
            static NSRunLoopCommonModes: *const NSRunLoopMode;
        }
        let mode: &NSRunLoopMode = &*NSRunLoopCommonModes;
        link.addToRunLoop_forMode(&runloop, mode);
    }

    eprintln!(
        "navi: CADisplayLink active on bg thread @ {} Hz (preferred range {}..{})",
        max_fps as u32, max_fps as u32, max_fps as u32
    );

    let raw_link = Retained::as_ptr(&link).cast_mut();
    let _ = LINK.set(LinkPtr(raw_link));

    // Hold the link Retained alive forever — it runs for the process lifetime.
    std::mem::forget(link);
    std::mem::forget(target);

    // Drive the run loop. `runMode:beforeDate:distantFuture` blocks until events
    // arrive (link ticks) and runs callbacks, then returns. We loop forever.
    loop {
        unsafe {
            let runloop = NSRunLoop::currentRunLoop();
            extern "C" {
                static NSDefaultRunLoopMode: *const NSRunLoopMode;
            }
            let mode: &NSRunLoopMode = &*NSDefaultRunLoopMode;
            // distantFuture so the loop only returns when a source fires.
            let until = NSDate::distantFuture();
            let _ran: bool = msg_send![&runloop, runMode: mode, beforeDate: &*until];
        }
    }
}

/// Read NSScreen on the bg thread. Apple permits this for read-only access on
/// modern macOS; we use `NSScreen.mainScreen` which returns the screen the
/// keyboard focus window is on (close enough for our purposes).
fn bg_screen() -> Option<Retained<NSScreen>> {
    // Avoid `MainThreadMarker::new()` — we're explicitly off-main here. Pull
    // mainScreen via runtime msg_send; on macOS 10.15+ this is documented to
    // work from any thread.
    unsafe {
        let cls = objc2::class!(NSScreen);
        let s: Option<Retained<NSScreen>> = msg_send![cls, mainScreen];
        s
    }
}

fn max_fps_from_window(window: &winit::window::Window) -> Option<f32> {
    let mtm = MainThreadMarker::new()?;
    let _ = mtm; // ensure we're on main when we touch NSWindow.screen
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };
    unsafe {
        let view: &objc2_app_kit::NSView =
            &*appkit.ns_view.cast::<objc2_app_kit::NSView>().as_ptr();
        let win = view.window()?;
        let screen = win.screen()?;
        let fps = screen.maximumFramesPerSecond();
        if fps > 0 {
            Some(fps as f32)
        } else {
            None
        }
    }
}
