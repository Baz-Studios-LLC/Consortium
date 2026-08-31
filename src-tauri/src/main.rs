// Consortium — the studio window.
//
// Consortium never launches or authenticates an agent. Claude Code and Codex
// already run, and are already signed in, inside their own apps. What they lack
// is a way to reach each other, so the project is a local message bus they all
// shell out to via the `consortium` CLI.
//
// This window is simply another participant: it reads the same append-only log
// the agents write to, and posts the user's own messages into it. There is no
// HTTP client here and no key storage anywhere in the project.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bus;

use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, State};

// ---------------------------------------------------------------------------
// Binary discovery
//
// A Finder-launched .app inherits a minimal PATH — /usr/bin:/bin:/usr/sbin:/sbin
// — so a CLI installed under /opt/homebrew/bin is invisible to a naive lookup.
// Only used to tell the user whether the agents can reach the CLI at all.
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
fn path_lookup(binary: &str) -> Option<PathBuf> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let out = Command::new(shell)
        .args(["-lc", &format!("command -v {}", binary)])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
    p.is_file().then_some(p)
}

#[cfg(windows)]
fn path_lookup(binary: &str) -> Option<PathBuf> {
    // No login shell to interrogate; `where` walks the same PATH the user sees.
    let out = Command::new("where").arg(binary).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let p = PathBuf::from(text.lines().next()?.trim().to_string());
    p.is_file().then_some(p)
}

#[cfg(not(windows))]
fn common_paths_lookup(binary: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    [
        format!("/opt/homebrew/bin/{}", binary),
        format!("/usr/local/bin/{}", binary),
        format!("{}/.local/bin/{}", home, binary),
        format!("{}/.cargo/bin/{}", home, binary),
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

#[cfg(windows)]
fn common_paths_lookup(_binary: &str) -> Option<PathBuf> {
    None // `where` already covers PATH, and there is no unix-style prefix to guess.
}

fn resolve_binary(binary: &str) -> Option<PathBuf> {
    path_lookup(binary).or_else(|| common_paths_lookup(binary))
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct Studio {
    workspace: Mutex<PathBuf>,
}

// ---------------------------------------------------------------------------
// Workspace
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_workspace(studio: State<Studio>) -> String {
    studio.workspace.lock().unwrap().to_string_lossy().into_owned()
}

#[tauri::command]
fn set_workspace(path: String, studio: State<Studio>) -> Result<String, String> {
    let p = PathBuf::from(&path);
    std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    *studio.workspace.lock().unwrap() = p.clone();
    Ok(p.to_string_lossy().into_owned())
}

#[derive(Serialize)]
struct FileEntry {
    name: String,
    is_dir: bool,
    size: u64,
}

/// The shared exchange folder — whatever one agent writes here, the others read.
#[tauri::command]
fn list_workspace_files(studio: State<Studio>) -> Vec<FileEntry> {
    let ws = studio.workspace.lock().unwrap().clone();
    let Ok(entries) = std::fs::read_dir(&ws) else {
        return Vec::new();
    };
    let mut files: Vec<FileEntry> = entries
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| {
            let meta = e.metadata().ok();
            FileEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                is_dir: meta.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
            }
        })
        .collect();
    files.sort_by(|a, b| (b.is_dir, a.name.to_lowercase()).cmp(&(a.is_dir, b.name.to_lowercase())));
    files
}

#[tauri::command]
fn reveal_workspace(studio: State<Studio>) -> Result<(), String> {
    let ws = studio.workspace.lock().unwrap().clone();
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let opener = "xdg-open";
    Command::new(opener).arg(&ws).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The message bus
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct BusMessage {
    from: String,
    text: String,
    /// Unix seconds; 0 for messages written before timestamps existed.
    at: u64,
}

#[derive(Serialize)]
struct Presence {
    who: String,
    /// "listening" (blocked on wait, will see new messages), "active", or "away"
    /// (turn ended — nothing can reach it until a human gives it a turn).
    state: String,
    age_secs: u64,
}

#[tauri::command]
fn bus_messages() -> Vec<BusMessage> {
    bus::read_lines()
        .iter()
        .filter_map(|l| {
            Some(BusMessage {
                from: bus::field(l, "from")?,
                text: bus::field(l, "text")?,
                at: bus::field(l, "at").and_then(|a| a.parse().ok()).unwrap_or(0),
            })
        })
        .collect()
}

#[tauri::command]
fn bus_presence() -> Vec<Presence> {
    bus::presence()
        .into_iter()
        .map(|(who, state, age_secs)| Presence { who, state, age_secs })
        .collect()
}

#[tauri::command]
fn bus_post(from: String, text: String) {
    bus::post(&from, &text);
}

/// Whether the agents can actually reach the CLI. Without it on PATH they have
/// no way to talk, so the UI needs to say so plainly.
#[tauri::command]
fn cli_installed() -> Option<String> {
    resolve_binary("consortium").map(|p| p.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// Self-update
//
// The UI polls on a long interval and shows a pill when a newer version is
// published. Installing is always an explicit click — an app that restarted
// itself would drop whatever the user was in the middle of saying.
// ---------------------------------------------------------------------------

#[tauri::command]
async fn update_check(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(Some(update.version.clone())),
        Ok(None) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn update_install(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(());
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart();
}

fn main() {
    let workspace = bus::workspace();
    let _ = std::fs::create_dir_all(&workspace);

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Studio {
            workspace: Mutex::new(workspace),
        })
        .invoke_handler(tauri::generate_handler![
            get_workspace,
            set_workspace,
            list_workspace_files,
            reveal_workspace,
            bus_messages,
            bus_presence,
            bus_post,
            cli_installed,
            update_check,
            update_install
        ])
        .run(tauri::generate_context!())
        .expect("error while running Consortium");
}
