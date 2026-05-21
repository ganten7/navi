# Navi

An interactive graph viewer for [org-roam](https://www.orgroam.com/), running as a native desktop window. No Emacs package required — Navi reads your `org-roam.db` directly and opens nodes in your existing Emacs process via `emacsclient`.

Written in Rust on top of [egui](https://github.com/emilk/egui) + [glow](https://github.com/grovesNL/glow) (OpenGL). On macOS it ships a hand-rolled vsync-aligned event loop that locks the renderer to the panel's native refresh rate (60, 120, 240 Hz) with sub-millisecond frame time variance, then drops to ~30 fps when idle to save power on laptops.

---

## Status

- **macOS** — primary target, fully supported. Tested on Apple Silicon (M-series) at 60 / 120 / 240 Hz. A pre-built `Navi.app` ships with each release.
- **Linux** — builds cleanly; the macOS-only `CADisplayLink` pacer is gated, so a small amount of glue (using `SwapInterval::Wait(1)` or Wayland presentation-time / DRM vblank) is required before it renders. Everything else (egui, winit, glutin, sqlite, emacsclient discovery) is cross-platform.
- **Windows** — not supported. The `emacsclient` socket discovery is Unix-only and the macOS pacer would have to be replaced by DXGI waitable swap chains.

---

## Quick start

### Pre-built macOS app (Apple Silicon)

Download `Navi-<version>-host.zip` from the [Releases](https://github.com/ganten7/navi/releases) page, unzip, and double-click `Navi.app`. The bundle is ad-hoc signed; on first launch macOS Gatekeeper may flag it — right-click → Open, or run `xattr -cr Navi.app` to clear the quarantine flag.

### From source

```bash
git clone https://github.com/ganten7/navi.git
cd navi
cargo build --release
./target/release/navi
```

To build the `.app` bundle yourself:

```bash
./scripts/build-macos.sh                # arm64 (host)
ARCH=universal ./scripts/build-macos.sh # arm64 + x86_64 universal
open dist/Navi.app
```

On first run, Navi auto-detects your `org-roam.db` and creates `~/.config/navi/config.json`. Subsequent launches start in well under a second.

---

## Requirements

- **Rust 1.75+** (stable) — install via [rustup](https://rustup.rs/) (source builds only)
- **org-roam v2** database (`nodes`, `files`, `links`, `tags`, `aliases`)
- **emacsclient** + a running Emacs server (`(server-start)` in your init) — for double-click-to-open
- A working OpenGL 3.3 context — built into macOS / standard on Linux

---

## Configuration

Config file: `~/.config/navi/config.json`. Created on first run with auto-detected defaults.

```json
{
  "db": "~/.emacs.d/org-roam.db",
  "emacsclient": "/opt/homebrew/bin/emacsclient",
  "server_name": "server",
  "show_fps": true
}
```

| Key | Description |
|---|---|
| `db` | Path to `org-roam.db`. Auto-detected from common Emacs / Doom / Spacemacs / XDG locations on first run. |
| `emacsclient` | Path to `emacsclient`. Bare names are resolved against Homebrew, MacPorts, `/usr/local/bin`, `/usr/bin`, `~/.local/bin`, `~/.nix-profile/bin`, NixOS, Snap, and `/Applications/Emacs.app`. |
| `server_name` | Emacs server name (default `server`). |
| `show_fps` | Show FPS counter in the status bar (`F` toggles at runtime). |

The legacy config path `~/.config/org-roam-graph/config.json` is also read; the next save writes to the new path.

### DB auto-detection order

| Path | Setup |
|---|---|
| `$ORG_ROAM_DB` | env override |
| `$XDG_DATA_HOME/emacs/org-roam.db` | XDG-strict Linux |
| `~/.emacs.d/org-roam.db` | vanilla Emacs |
| `~/.config/emacs/org-roam.db` | XDG-style Emacs |
| `~/.config/doom/.local/etc/org-roam.db` | Doom 3.x |
| `~/.config/doom/org-roam.db` | Doom 3.x fallback |
| `~/.doom.d/.local/etc/org-roam.db` | Doom 2.x |
| `~/.doom.d/org-roam.db` | Doom 2.x fallback |
| `~/.spacemacs.d/org-roam.db` | Spacemacs |

---

## Opening nodes in Emacs

Double-click a node (or select it and press `Enter` / `Space`). File nodes open the file; **headline nodes jump to the heading** via `goto-char`.

GUI apps on macOS get a minimal `PATH`, so Navi resolves `emacsclient` to an absolute path and probes the server socket under:

- `$EMACS_SERVER_SOCKET` / `$EMACS_SERVER_FILE`
- `$XDG_RUNTIME_DIR/emacs/`
- `$TMPDIR/emacs{uid}/`
- `/tmp`, `/private/tmp`
- `/var/folders/*/*/T/emacs{uid}/` (macOS GUI vs Terminal `TMPDIR` mismatch)

If open fails, an error appears in the status bar. Make sure Emacs has `(server-start)` in its init.

---

## Controls

| Input | Action |
|---|---|
| Drag background | Pan view |
| Swipe + release | Kinetic pan (momentum) |
| Drag node | Move node |
| Scroll / trackpad | Zoom toward cursor |
| Click node | Select — highlights connections |
| Double-click node | Open in Emacs |
| `Tab` / `Shift-Tab` | Cycle nodes |
| `Enter` / `Space` | Open selected node |
| `T` | Cycle colour theme |
| `G` | Toggle tag colouring |
| `A` | Toggle age / weathering heatmap |
| `D` | Toggle daily-notes filter |
| `O` | Toggle orphan filter |
| `L` | Cycle local-graph mode (1 → 2 → 3 hops → off) |
| `V` | Cycle layout algorithm |
| `/` | Search by title or alias |
| `W` | Reload graph from database |
| `F` | Toggle FPS counter |
| `P` | Pause / resume physics |
| `R` | Reset view |
| `H` | Hold to show controls panel |
| `Q` / `Escape` | Quit |

---

## Frame pacing

On macOS Navi runs a hand-rolled event loop:

- A dedicated background thread owns a `CADisplayLink` pinned to the panel's max refresh rate via `CAFrameRateRange`. On each vsync it sends a `UserEvent::Vsync` to the main thread via winit's `EventLoopProxy`.
- Mouse / keyboard events do **not** trigger paints. Display vsync ticks are the sole paint signal — this eliminates the multi-paint-per-frame burn that input-driven loops produce.
- After ~10 s of inactivity (configurable with `NAVI_IDLE_GRACE_SECS`), the link pauses and the OS drops the panel to its lowest tier. The next input or focus event resumes the link within a frame.
- OpenGL's `swap_buffers` is called with `SwapInterval::DontWait` — macOS GL caps swap-vsync at ~108 Hz on ProMotion panels regardless of the displayed tier, so the display-link does the pacing instead.

Result on a 240 Hz display: 4.17 ms average frame interval, worst-case 5–7 ms (no missed vsyncs in steady state), and the application sits near 0 % CPU when idle.

### Diagnostics

| Env var | Effect |
|---|---|
| `NAVI_FPS_LOG=1` | Print fps + frame stats + display-link metadata to stderr every ~1 s |
| `NAVI_PROF=1` | Print per-layer paint timing (grid/edges/nodes/labels/help) every ~1 s |
| `NAVI_IDLE_GRACE_SECS=N` | Override the 10 s active-after-idle grace window |
| `NAVI_NO_GRID`, `NAVI_NO_EDGES`, `NAVI_NO_NODES`, `NAVI_NO_LABELS` | Disable individual paint layers (perf debugging) |

---

## Project layout

```
Cargo.toml                Workspace root
navi-core/                Pure-Rust library: config, org-roam DB loader, graph + physics
  src/lib.rs
  src/config.rs           Config load/save, db detection, path expansion
  src/emacs.rs            emacsclient + socket discovery
  src/graph.rs            Force-directed physics, layouts, hidden/faded sets
navi/                     Binary: UI, rendering, event loop
  src/main.rs             winit + glutin event loop, paint pacer
  src/app.rs              egui app: input, layout, status bar
  src/painter.rs          GraphPainter — grid, edges, nodes, labels
  src/macos_display.rs    macOS CADisplayLink + tier control
  src/theme.rs            Colour themes
scripts/build-macos.sh    Cargo build + Navi.app + zip for release
assets/icon.icns          App bundle icon
```

---

## Building

```bash
# Release (recommended for use)
cargo build --release

# Dev (slower at runtime, faster compile, includes debuginfo)
cargo build

# macOS .app bundle + release zip
./scripts/build-macos.sh
```

The release profile in the workspace has `lto = true` and `opt-level = 3`. Expect a 30–60 s clean release build on a modern laptop.

---

## License

Source release. Add a `LICENSE` file if you intend to distribute binaries.
