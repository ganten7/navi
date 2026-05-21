use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use crate::graph::Node;

static EMACSCLIENT_CANDIDATES: &[&str] = &[
    "emacsclient",
    "/opt/homebrew/bin/emacsclient",
    "/usr/local/bin/emacsclient",
    "/usr/bin/emacsclient",
    "/opt/local/bin/emacsclient",
    "/Applications/Emacs.app/Contents/MacOS/bin/emacsclient",
    "/Applications/Emacs.app/Contents/MacOS/emacsclient",
    "/run/current-system/sw/bin/emacsclient",
    "/snap/bin/emacsclient",
    "~/.local/bin/emacsclient",
    "~/.nix-profile/bin/emacsclient",
];

pub struct EmacsClient {
    pub binary: String,
    pub server_name: String,
}

impl EmacsClient {
    pub fn new(cfg_binary: &str, server_name: &str) -> Self {
        let binary = if !cfg_binary.is_empty() && Path::new(cfg_binary).exists() {
            cfg_binary.to_string()
        } else {
            detect_emacsclient()
        };
        EmacsClient { binary, server_name: server_name.to_string() }
    }

    pub fn open_node(&self, node: &Node) -> Result<(), String> {
        if node.file.is_empty() || !Path::new(&node.file).exists() {
            return Err(format!("File not found: {}", node.file));
        }

        let sock = find_emacs_socket(&self.server_name);
        let mut cmd_args: Vec<String> = Vec::new();
        if let Some(s) = &sock {
            cmd_args.push("--socket-name".into());
            cmd_args.push(s.clone());
        }
        cmd_args.push("--no-wait".into());
        cmd_args.push("--alternate-editor=".into());
        cmd_args.push("--eval".into());

        let path = node.file.replace('\\', "\\\\").replace('"', "\\\"");
        let goto = if node.level > 0 && node.pos > 0 {
            format!(" (goto-char {})", node.pos)
        } else {
            String::new()
        };
        let elisp = format!(
            "(progn (find-file \"{path}\"){goto} (delete-other-windows) (when (display-graphic-p) (raise-frame)))"
        );
        cmd_args.push(elisp);

        Command::new(&self.binary)
            .args(&cmd_args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("emacsclient failed: {e}"))
    }
}

fn detect_emacsclient() -> String {
    for cand in EMACSCLIENT_CANDIDATES {
        let expanded = if let Some(rest) = cand.strip_prefix("~/") {
            format!("{}/{}", dirs::home_dir().unwrap_or_default().display(), rest)
        } else {
            cand.to_string()
        };
        if Path::new(&expanded).exists() {
            return expanded;
        }
        // Try which
        if let Ok(out) = Command::new("which").arg(cand).output() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() && Path::new(&s).exists() {
                return s;
            }
        }
    }
    "emacsclient".to_string()
}

fn find_emacs_socket(server_name: &str) -> Option<String> {
    for key in &["EMACS_SERVER_SOCKET", "EMACS_SERVER_FILE"] {
        if let Ok(v) = std::env::var(key) {
            if Path::new(&v).exists() {
                return Some(v);
            }
        }
    }

    let uid = unsafe { libc_getuid() };
    let names: Vec<&str> = if server_name != "server" {
        vec![server_name, "server"]
    } else {
        vec!["server"]
    };

    // XDG_RUNTIME_DIR (Linux)
    if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        for name in &names {
            let p = format!("{}/emacs/{}", xdg, name);
            if Path::new(&p).exists() { return Some(p); }
        }
    }

    // macOS temp dirs
    let bases: Vec<PathBuf> = {
        let mut b = Vec::new();
        for key in &["TMPDIR", "TMP", "TEMP"] {
            if let Ok(v) = std::env::var(key) {
                b.push(PathBuf::from(v));
            }
        }
        b.push(PathBuf::from("/tmp"));
        b.push(PathBuf::from("/private/tmp"));
        b
    };

    for base in bases {
        for name in &names {
            let p = base.join(format!("emacs{}", uid)).join(name);
            if p.exists() { return Some(p.to_string_lossy().into_owned()); }
        }
    }
    None
}

#[cfg(unix)]
fn libc_getuid() -> u32 {
    unsafe { libc_uid() }
}

#[cfg(not(unix))]
fn libc_getuid() -> u32 { 0 }

#[cfg(unix)]
extern "C" { fn getuid() -> u32; }

#[cfg(unix)]
unsafe fn libc_uid() -> u32 { getuid() }
