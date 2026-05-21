use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub db: String,
    #[serde(default)]
    pub emacsclient: String,
    #[serde(default = "default_server")]
    pub server_name: String,
    #[serde(default = "default_true")]
    pub show_fps: bool,
    #[serde(default)]
    pub borderless: bool,
}

fn default_server() -> String { "server".into() }
fn default_true() -> bool { true }

impl Default for Config {
    fn default() -> Self {
        Config {
            db: String::new(),
            emacsclient: String::new(),
            server_name: "server".into(),
            show_fps: true,
            borderless: false,
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_default()
            .join(".config")
            .join("navi")
            .join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(s) = std::fs::read_to_string(&path) {
                if let Ok(c) = serde_json::from_str(&s) {
                    return c;
                }
            }
        }
        Config::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, s);
        }
    }
}

// ── DB path detection ─────────────────────────────────────────────────────────

static DB_CANDIDATES: &[&str] = &[
    "~/.emacs.d/org-roam.db",
    "~/.config/emacs/org-roam.db",
    "~/.config/doom/.local/etc/org-roam.db",
    "~/.config/doom/org-roam.db",
    "~/.doom.d/.local/etc/org-roam.db",
    "~/.doom.d/org-roam.db",
    "~/.spacemacs.d/org-roam.db",
];

pub fn detect_db() -> String {
    if let Ok(v) = std::env::var("ORG_ROAM_DB") {
        if std::path::Path::new(&v).exists() {
            return v;
        }
    }
    // XDG_DATA_HOME
    let xdg = std::env::var("XDG_DATA_HOME")
        .unwrap_or_else(|_| format!("{}/.local/share", dirs::home_dir().unwrap_or_default().display()));
    let xdg_db = format!("{}/emacs/org-roam.db", xdg);
    if std::path::Path::new(&xdg_db).exists() {
        return xdg_db;
    }
    for cand in DB_CANDIDATES {
        let expanded = expand_tilde(cand);
        if std::path::Path::new(&expanded).exists() {
            return expanded;
        }
    }
    expand_tilde(DB_CANDIDATES[0])
}

pub fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = dirs::home_dir().unwrap_or_default();
        return format!("{}/{}", home.display(), rest);
    }
    path.to_string()
}
