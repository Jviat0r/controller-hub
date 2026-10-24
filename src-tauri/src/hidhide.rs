//! Optional HidHide integration through its command-line tool. HidHide hides
//! the real PlayStation HID device from every process except the ones on its
//! allow-list, so games only see our virtual Xbox pad and never get double
//! input. Nothing here runs unless HidHide is installed and the setting is on.

use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn cli_path() -> Option<PathBuf> {
    let pf = std::env::var_os("ProgramFiles").map(PathBuf::from)?;
    let root = pf.join("Nefarius Software Solutions").join("HidHide");
    [root.join("x64").join("HidHideCLI.exe"), root.join("HidHideCLI.exe")]
        .into_iter()
        .find(|p| p.exists())
}

pub fn installed() -> bool {
    cli_path().is_some()
}

fn run(args: &[&str]) -> Result<String, String> {
    let cli = cli_path().ok_or_else(|| "HidHide is not installed".to_string())?;
    let out = Command::new(cli)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(format!(
            "HidHideCLI {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// `Some(true)` when HidHide is currently cloaking devices.
pub fn is_active() -> Option<bool> {
    run(&["--cloak-state"]).ok().map(|t| t.to_ascii_lowercase().contains("cloak-on"))
}

pub fn register_app(exe: &Path) -> Result<(), String> {
    run(&["--app-reg", &exe.to_string_lossy()]).map(|_| ())
}

pub fn cloak_on() -> Result<(), String> {
    run(&["--cloak-on"]).map(|_| ())
}

pub fn hide(instance_path: &str) -> Result<(), String> {
    run(&["--dev-hide", instance_path]).map(|_| ())
}

pub fn unhide(instance_path: &str) -> Result<(), String> {
    run(&["--dev-unhide", instance_path]).map(|_| ())
}
