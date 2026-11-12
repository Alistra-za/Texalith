//! The pinned Community Shaders copy: where it is, its settings and its pin.
use render3d::cs_settings::{read_settings, CsSettings, SettingsPaths};
use std::path::{Path, PathBuf};

/// `cs-shaders` beside the exe in a release, `assets/cs-shaders` when run from the repo.
pub fn dir() -> PathBuf {
    let released = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.join("cs-shaders")));
    released
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/cs-shaders"))
}

pub fn shaders(dir: &Path) -> PathBuf {
    dir.join("shaders")
}

/// Pinned settings, never the user's: every preview looks the same on every machine.
pub fn settings(dir: &Path) -> CsSettings {
    read_settings(&SettingsPaths {
        user: Some(dir.join("settings/SettingsUser.json")),
        default: None,
        info_ini: Some(dir.join("settings/Info.ini")),
        features_dir: None,
    })
}

/// A `key = "value"` line of `PIN.toml`.
pub fn pin(dir: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("PIN.toml")).ok()?;
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// Per-user folder for compiled shaders. The key includes the source hash, so a re-pin recompiles by itself.
pub fn cache_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("Texalith/shader-cache")
}
