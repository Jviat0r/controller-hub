//! Settings file: %APPDATA%\ControllerHub\settings.json

use std::{fs, path::PathBuf};

use controller_hub_core::model::Settings;

pub fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("ControllerHub")
}

pub fn path() -> PathBuf {
    dir().join("settings.json")
}

pub fn load() -> Settings {
    match fs::read_to_string(path()) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

/// Write to a temp file then rename, so a crash mid-write never leaves a
/// half-written settings file.
pub fn save(settings: &Settings) -> Result<(), String> {
    let d = dir();
    fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let tmp = d.join("settings.json.tmp");
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path()).map_err(|e| e.to_string())
}
