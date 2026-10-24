//! Tauri shell: window, tray icon, and the thin command layer that forwards
//! UI intent to the engine.

pub mod bluetooth;
pub mod drivers;
mod engine;
pub mod hid;
mod hidhide;
mod settings;
pub mod vigem;
pub mod xinput;

use controller_hub_core::{model::Snapshot, tester::Layout};
use engine::{Cmd, EngineHandle};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, RunEvent, State, WindowEvent,
};

type Engine<'a> = State<'a, EngineHandle>;

#[tauri::command]
fn get_snapshot(engine: Engine<'_>) -> Snapshot {
    engine.snapshot()
}

#[tauri::command]
fn reassign(engine: Engine<'_>, id: String, slot: usize) -> Result<(), String> {
    engine.send(Cmd::Reassign { id, slot })
}

#[tauri::command]
fn set_color(engine: Engine<'_>, id: String, rgb: [u8; 3]) -> Result<(), String> {
    engine.send(Cmd::SetColor { id, rgb })
}

#[tauri::command]
fn set_light_off(engine: Engine<'_>, id: String, off: bool) -> Result<(), String> {
    engine.send(Cmd::SetLightOff { id, off })
}

#[tauri::command]
fn identify(engine: Engine<'_>, id: String) -> Result<(), String> {
    engine.send(Cmd::Identify { id })
}

#[tauri::command]
fn start_probe(engine: Engine<'_>, id: String) -> Result<Layout, String> {
    engine.start_probe(id)
}

#[tauri::command]
fn stop_probe(engine: Engine<'_>) -> Result<(), String> {
    engine.send(Cmd::StopProbe)
}

#[tauri::command]
fn reset_probe(engine: Engine<'_>) -> Result<(), String> {
    engine.send(Cmd::ResetProbe)
}

#[tauri::command]
fn rename(engine: Engine<'_>, id: String, name: String) -> Result<(), String> {
    engine.send(Cmd::Rename { id, name })
}

#[tauri::command]
fn release_hold(engine: Engine<'_>, slot: usize) -> Result<(), String> {
    engine.send(Cmd::ReleaseHold { slot })
}

#[tauri::command]
fn bt_scan(engine: Engine<'_>) -> Result<(), String> {
    engine.send(Cmd::BtScan)
}

#[tauri::command]
fn bt_pair(engine: Engine<'_>, id: String) -> Result<(), String> {
    engine.send(Cmd::BtPair { id })
}

#[tauri::command]
fn bt_unpair(engine: Engine<'_>, id: String) -> Result<(), String> {
    engine.send(Cmd::BtUnpair { id })
}

#[tauri::command]
fn set_setting(engine: Engine<'_>, key: String, value: serde_json::Value) -> Result<(), String> {
    engine.send(Cmd::SetSetting { key, value })
}

#[tauri::command]
fn recheck_drivers(engine: Engine<'_>) -> Result<(), String> {
    engine.send(Cmd::RecheckDrivers)
}

#[tauri::command]
fn open_url(engine: Engine<'_>, key: String) -> Result<(), String> {
    engine.send(Cmd::OpenUrl { key })
}

#[tauri::command]
fn install_driver(engine: Engine<'_>, key: String) -> Result<(), String> {
    engine.send(Cmd::InstallDriver { key })
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn quit(app: &AppHandle) {
    if let Some(engine) = app.try_state::<EngineHandle>() {
        engine.shutdown();
    }
    app.exit(0);
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            reassign,
            set_color,
            set_light_off,
            identify,
            start_probe,
            stop_probe,
            reset_probe,
            rename,
            release_hold,
            bt_scan,
            bt_pair,
            bt_unpair,
            set_setting,
            recheck_drivers,
            open_url,
            install_driver
        ])
        .setup(|app| {
            let engine = engine::start(app.handle().clone());
            let start_minimized = engine.snapshot().settings.start_minimized;
            app.manage(engine);

            let show = MenuItem::with_id(app, "show", "Show Controller Hub", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit_item])?;
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("Controller Hub")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => quit(app),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            if start_minimized {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            Ok(())
        })
        // Closing the window keeps the hub (and every controller) alive in the tray.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Controller Hub")
        .run(|app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                if let Some(engine) = app.try_state::<EngineHandle>() {
                    engine.shutdown();
                }
            }
        });
}
