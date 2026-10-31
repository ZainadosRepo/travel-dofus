mod settings;

use dofus_data::extract::{default_game_dir, installed_version};
use dofus_data::route::{Atlas, Plan, ZaapStop};
use dofus_data::search::{parse_coords, Place, Searcher};
use dofus_data::world::{Lang, World};
use serde::Serialize;
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Extracted by `cargo run -p dofus-data --bin extract`; checked by `crates/dofus-data/tests/world_data.rs`.
const WORLD_JSON: &str = include_str!("../../data/world.json");

struct AppState {
    atlas: Atlas,
    searcher: Searcher,
    /// Set when Ctrl + Alt + P could not be registered (already taken by another app).
    shortcut_error: OnceLock<String>,
}

impl AppState {
    fn distances(&self, from: Option<&str>) -> Option<Vec<Option<u32>>> {
        let (x, y) = parse_coords(from?)?;
        self.atlas
            .distances_from(self.atlas.surface_map_at(x, y)?.id)
    }
}

#[tauri::command]
fn search(
    state: tauri::State<AppState>,
    query: String,
    from: Option<String>,
    lang: Lang,
) -> Vec<Place> {
    let from = from.as_deref().and_then(parse_coords);
    state.searcher.search(&state.atlas, &query, from, lang)
}

/// Route to bare coordinates: custom positions and corrected NPCs.
#[tauri::command]
fn plan_at(
    state: tauri::State<AppState>,
    x: i32,
    y: i32,
    from: Option<String>,
    lang: Lang,
) -> Option<Plan> {
    let distances = state.distances(from.as_deref());
    state.atlas.plan_at(x, y, distances.as_deref(), lang)
}

#[tauri::command]
fn zaaps(state: tauri::State<AppState>, lang: Lang) -> Vec<ZaapStop> {
    state.atlas.zaaps(lang)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DataStatus {
    data_version: String,
    /// `None` when no Dofus install is found where the launcher puts it.
    installed_version: Option<String>,
    npcs: usize,
    zaaps: usize,
    markers: usize,
    shortcut_error: Option<String>,
}

#[tauri::command]
fn data_status(state: tauri::State<AppState>) -> DataStatus {
    let world = &state.atlas.world;
    DataStatus {
        data_version: world.game_version.clone(),
        installed_version: default_game_dir().and_then(|dir| installed_version(&dir).ok()),
        npcs: world.npcs.len(),
        zaaps: world.zaaps.len(),
        markers: world.markers.len(),
        shortcut_error: state.shortcut_error.get().cloned(),
    }
}

fn settings_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json"))
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<settings::Loaded, String> {
    settings::load(&settings_path(&app)?)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: serde_json::Value) -> Result<(), String> {
    settings::save(&settings_path(&app)?, &settings)
}

/// Ctrl + Alt + P: hide the window, or bring it back in front of the game.
fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let shown = window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false);
    let result = if shown {
        window.hide()
    } else {
        window
            .unminimize()
            .and_then(|_| window.show())
            .and_then(|_| window.set_focus())
    };
    if let Err(e) = result {
        eprintln!("Ctrl+Alt+P: {e}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let world: World = serde_json::from_str(WORLD_JSON).expect("embedded world.json is valid");
    let atlas = Atlas::new(world);
    let searcher = Searcher::new(&atlas);
    let toggle = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyP);
    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut == &toggle && event.state() == ShortcutState::Pressed {
                        toggle_main_window(app);
                    }
                })
                .build(),
        )
        .manage(AppState {
            atlas,
            searcher,
            shortcut_error: OnceLock::new(),
        })
        .setup(move |app| {
            if let Err(e) = app.global_shortcut().register(toggle) {
                let _ = app.state::<AppState>().shortcut_error.set(e.to_string());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            search,
            plan_at,
            zaaps,
            data_status,
            load_settings,
            save_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
