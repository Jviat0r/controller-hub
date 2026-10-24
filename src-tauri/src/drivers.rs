//! Bundled driver installers at run time: find them next to the app, verify
//! them against the manifest, and run one elevated when the user asks.

use std::{
    fs,
    path::{Path, PathBuf},
};

use controller_hub_core::bundle::{verify, InstallerSpec};
use windows::{
    core::{HSTRING, PCWSTR},
    Win32::{
        Foundation::CloseHandle,
        System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE},
        UI::{
            Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW},
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleState {
    Verified,
    Missing,
    Corrupt(String),
}

pub fn installer_path(resource_dir: &Path, spec: &InstallerSpec) -> PathBuf {
    resource_dir.join("drivers").join(&spec.file)
}

/// Byte-exact check of the bundled file against the manifest, every time.
pub fn check(resource_dir: &Path, spec: &InstallerSpec) -> BundleState {
    match fs::read(installer_path(resource_dir, spec)) {
        Ok(bytes) => match verify(spec, &bytes) {
            Ok(()) => BundleState::Verified,
            Err(e) => BundleState::Corrupt(e),
        },
        Err(_) => BundleState::Missing,
    }
}

/// Run an installer through UAC ("runas") and wait for its exit code.
/// 0 = done, 3010 = done but Windows wants a restart, 1602 = user cancelled.
pub fn run_elevated(path: &Path, args: &str) -> Result<u32, String> {
    let file = HSTRING::from(path.as_os_str());
    let params = HSTRING::from(args);
    let verb = HSTRING::from("runas");
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe {
        ShellExecuteExW(&mut info).map_err(|e| format!("{e}"))?;
        if info.hProcess.is_invalid() {
            return Err("the installer did not start".into());
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 0u32;
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = CloseHandle(info.hProcess);
        Ok(code)
    }
}

pub fn describe_exit(code: u32) -> String {
    match code {
        0 => "installed".into(),
        3010 => "installed; Windows wants a restart before it works".into(),
        1602 | 1223 => "cancelled".into(),
        1603 => "failed (installer error 1603)".into(),
        other => format!("finished with code {other}"),
    }
}
