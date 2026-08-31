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

mod agent;
mod bus;
mod claude_adapter;
mod conversation;
mod codex_adapter;
mod manager;
mod router;
mod sessions;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use serde::Serialize;
use notify::RecommendedWatcher;
use tauri::{AppHandle, Emitter, Manager, State};

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
    /// Kept alive deliberately: a dropped watcher stops watching, so one that
    /// is not held somewhere quietly does nothing at all.
    watcher: Mutex<Option<RecommendedWatcher>>,
    /// An update that has been fetched and is waiting for the app to close.
    pending_update: Mutex<Option<PendingUpdate>>,
    /// Wakes agents when the room changes. None if no agent could be started,
    /// in which case the room still works — it just has nobody to wake.
    agents: Mutex<Option<std::sync::Arc<manager::AgentManager>>>,
}

/// A downloaded update, held until the app exits.
///
/// Downloading and installing are separated on purpose. Installing restarts the
/// app, and this is a chat window — restarting it mid-sentence would throw away
/// whatever the user was typing. Applying the update on the way out costs them
/// nothing and asks them nothing.
struct PendingUpdate {
    version: String,
    update: tauri_plugin_updater::Update,
    bytes: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Workspace
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_workspace(studio: State<Studio>) -> String {
    studio.workspace.lock().unwrap().to_string_lossy().into_owned()
}

#[tauri::command]
fn set_workspace(
    path: String,
    app: AppHandle,
    studio: State<Studio>,
) -> Result<String, String> {
    let p = PathBuf::from(&path);
    std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
    *studio.workspace.lock().unwrap() = p.clone();

    // Re-point the watcher, or the room would keep listening to the directory
    // it was moved away from and go quiet without appearing to.
    match watch_workspace(app, p.clone()) {
        Ok(w) => *studio.watcher.lock().unwrap() = Some(w),
        Err(e) => bus::log(&format!("could not watch {}: {e}", p.display())),
    }
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
    /// Who the message explicitly addressed, lowercased. Empty means it
    /// addressed nobody — which is not the same as addressing everybody, and is
    /// what keeps two agents from answering each other indefinitely.
    to: Vec<String>,
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

/// A slice of a room's transcript, and how long the whole thing is.
#[derive(Serialize)]
struct MessagePage {
    /// Total messages in the room, so the window can tell what it has not
    /// asked for yet without being sent it.
    total: usize,
    messages: Vec<BusMessage>,
}

/// Everything said after `from`.
///
/// Used to return the whole room every time, which was fine until a room had
/// eight thousand messages in it: parsing and serialising all of them ran on
/// every poll and every file change, and froze the window for twenty seconds
/// at launch. Only new lines are parsed now.
#[tauri::command]
fn bus_messages_since(from: usize) -> MessagePage {
    let lines = bus::read_lines();
    let total = lines.len();
    // Clamped rather than trusted: the window's idea of where it had got to
    // is stale the moment a room is cleared.
    let start = from.min(total);

    let messages = lines[start..]
        .iter()
        .filter_map(|l| {
            Some(BusMessage {
                from: bus::field(l, "from")?,
                text: bus::field(l, "text")?,
                to: bus::field(l, "to")
                    .unwrap_or_default()
                    .split(',')
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(str::to_string)
                    .collect(),
                at: bus::field(l, "at").and_then(|a| a.parse().ok()).unwrap_or(0),
            })
        })
        .collect();

    MessagePage { total, messages }
}

#[tauri::command]
fn bus_presence() -> Vec<Presence> {
    bus::presence()
        .into_iter()
        .map(|(who, state, age_secs)| Presence { who, state, age_secs })
        .collect()
}

/// What each agent Consortium runs is actually doing.
///
/// Distinct from `bus_presence`, which describes the older arrangement where
/// an agent sat blocked on `consortium wait` and could only be reached during
/// its own turn. An agent Consortium starts is reachable whenever it is up,
/// so "away" stops being a meaningful thing to say about it.
///
/// Empty when no agent could be started — the UI falls back to presence,
/// which is still correct for anyone joining the room by hand.
#[derive(Serialize)]
struct AgentStatus {
    who: String,
    /// offline | starting | idle | working | error
    state: String,
    /// Why, when the state is an error. Carried separately so the UI can show
    /// the reason without parsing it back out of a label.
    detail: Option<String>,
}

#[tauri::command]
fn agent_states(studio: tauri::State<Studio>) -> Vec<AgentStatus> {
    // Cloned out of the lock: asking an adapter for its state takes the
    // adapter's own lock, and holding Studio across that invites a deadlock
    // with a turn that is finishing at the same moment.
    let manager = studio.agents.lock().unwrap().clone();
    let Some(manager) = manager else {
        return Vec::new();
    };

    manager
        .states()
        .into_iter()
        .map(|(who, state)| {
            let detail = match &state {
                agent::AgentState::Error(why) => Some(why.clone()),
                _ => None,
            };
            let label = match state {
                agent::AgentState::Offline => "offline",
                agent::AgentState::Starting => "starting",
                agent::AgentState::Idle => "idle",
                agent::AgentState::Working => "working",
                agent::AgentState::Error(_) => "error",
            };
            AgentStatus { who, state: label.to_string(), detail }
        })
        .collect()
}

/// Empties the room and tells the manager to start over.
///
/// Both, or neither. Archiving the file while the manager still believes it
/// is 8000 lines in would leave a room that accepts messages and wakes nobody
/// — working, to look at, and inert.
#[tauri::command]
fn bus_clear(studio: tauri::State<Studio>) -> Result<String, String> {
    let room = conversation::active();
    let archived = bus::archive_for(&room)?;
    if let Some(manager) = studio.agents.lock().unwrap().clone() {
        manager.reset(&room);
    }
    Ok(archived.display().to_string())
}

/// The rooms, and which one the window is showing.
#[tauri::command]
fn conversations() -> Vec<conversation::Conversation> {
    conversation::list()
}

#[tauri::command]
fn conversation_active() -> String {
    conversation::active()
}

#[tauri::command]
fn conversation_select(slug: String) -> Result<(), String> {
    conversation::set_active(&slug)
}

/// Adds a room, optionally tied to a project folder.
///
/// The folder is where agents woken here will work, and where their sessions
/// live. Left empty it is the Consortium workspace, which is right for a room
/// that is not about a particular repository.
#[tauri::command]
fn conversation_create(name: String, dir: Option<String>) -> Result<conversation::Conversation, String> {
    let dir = dir
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .map(std::path::PathBuf::from);

    // Refused here rather than accepted and quietly ignored later: a room
    // pointed at a folder that does not exist would fall back to the shared
    // workspace, and every agent woken in it would be in the wrong place with
    // nothing to say why.
    if let Some(d) = &dir {
        if !d.is_dir() {
            return Err(format!("{} is not a folder on this machine", d.display()));
        }
    }
    conversation::create(&name, dir)
}

/// Changes where a room works. Empty puts it back in the shared workspace.
#[tauri::command]
fn conversation_set_dir(slug: String, dir: Option<String>) -> Result<(), String> {
    let dir = dir
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .map(std::path::PathBuf::from);

    if let Some(d) = &dir {
        if !d.is_dir() {
            return Err(format!("{} is not a folder on this machine", d.display()));
        }
    }
    conversation::set_dir(&slug, dir)
}

/// Claude Code sessions on this machine, for choosing one to continue.
#[tauri::command]
fn claude_sessions() -> Vec<sessions::SessionInfo> {
    sessions::list()
}

/// Points a room's agent at a session that already exists.
///
/// The directory is checked because sessions are scoped to one: attaching a
/// session held somewhere else produces a room that looks configured and
/// fails on its first wake with 'No conversation found'. Better to refuse now
/// and say why.
#[tauri::command]
fn conversation_attach(
    slug: String,
    agent: String,
    session: String,
    session_dir: String,
) -> Result<(), String> {
    let room_dir = conversation::dir_for(&slug);
    let same = std::fs::canonicalize(&room_dir)
        .ok()
        .zip(std::fs::canonicalize(&session_dir).ok())
        .map(|(a, b)| a == b)
        .unwrap_or(false);

    if !same {
        return Err(format!(
            "That session was held in {session_dir}, and this room works in {}. \
             Claude Code can only resume a session from the folder it was held in, \
             so point the room at that folder first.",
            room_dir.display()
        ));
    }
    conversation::attach_session(&slug, &agent, &session)
}

#[tauri::command]
fn bus_post(from: String, text: String) {
    bus::post(&from, &text);
}

/// What version is actually running.
///
/// Worth showing now that updates apply silently on exit: an app that changes
/// underneath you needs somewhere to say what it currently is, or "have you got
/// the fix yet" becomes a question nobody can answer.
#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Whether the agents can actually reach the CLI. Without it on PATH they have
/// no way to talk, so the UI needs to say so plainly.
#[tauri::command]
fn cli_installed() -> Option<String> {
    resolve_binary("consortium").map(|p| p.to_string_lossy().into_owned())
}

/// Where to put the CLI. Prefer a directory the user's login shell already has on
/// PATH — installing somewhere invisible looks like success and then fails at the
/// first `consortium post`, which is the worst possible outcome.
fn cli_install_dir() -> (PathBuf, bool) {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());

    let shell_path = {
        #[cfg(not(windows))]
        {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
            Command::new(shell)
                .args(["-lc", "echo $PATH"])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default()
        }
        #[cfg(windows)]
        { std::env::var("PATH").unwrap_or_default() }
    };

    let candidates = [
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from(&home).join(".local/bin"),
        PathBuf::from(&home).join(".cargo/bin"),
    ];

    // First choice: already on PATH and writable without a password.
    for dir in &candidates {
        let on_path = path_lists(&shell_path, dir);
        let writable = dir.is_dir()
            && std::fs::OpenOptions::new()
                .write(true).create(true).truncate(true)
                .open(dir.join(".consortium-write-test"))
                .map(|_| { let _ = std::fs::remove_file(dir.join(".consortium-write-test")); true })
                .unwrap_or(false);
        if on_path && writable {
            return (dir.clone(), true);
        }
    }

    // Fall back to ~/.local/bin and tell the caller it is not on PATH yet.
    let fallback = PathBuf::from(&home).join(".local/bin");
    let on_path = path_lists(&shell_path, &fallback);
    (fallback, on_path)
}

/// Whether a PATH string actually lists a directory.
///
/// Split with `std::env::split_paths` rather than on a hard-coded separator.
/// This used to split on ':', which is correct on Unix and nonsense on Windows:
/// the separator there is ';' and the paths themselves contain colons, so
/// "C:\Users\me\.local\bin;C:\Windows" split on ':' yields "C",
/// "\Users\me\.local\bin;C" and "\Windows" — fragments that can
/// never equal a real directory. The result was that on Windows this always
/// answered "no", so the app told people to add a folder to their PATH that was
/// already on it.
///
/// Comparison is case-insensitive on Windows, where two spellings of the same
/// directory are the same directory.
fn path_lists(path_var: &str, dir: &Path) -> bool {
    /// A trailing separator and, on Windows, letter case are both differences
    /// that do not make it a different directory.
    fn normalise(p: &Path) -> String {
        let text = p.to_string_lossy();
        let trimmed = text.trim_end_matches(std::path::MAIN_SEPARATOR);
        if cfg!(windows) {
            trimmed.to_lowercase()
        } else {
            trimmed.to_string()
        }
    }

    let target = normalise(dir);
    std::env::split_paths(path_var).any(|entry| normalise(&entry) == target)
}

#[derive(Serialize)]
struct InstallResult {
    path: String,
    /// False when the install directory is not on the user's PATH, in which case
    /// the agents still will not be able to run it.
    on_path: bool,
}

/// Copy the bundled CLI somewhere the agents can actually invoke it. Without this
/// a downloaded app is inert: the briefings tell each agent to run `consortium`,
/// and there is no `consortium`.
#[tauri::command]
fn install_cli(app: AppHandle) -> Result<InstallResult, String> {
    let exe = if cfg!(windows) { "consortium.exe" } else { "consortium" };

    let src = app
        .path()
        .resolve(format!("resources/{}", exe), tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("bundled CLI not found: {e}"))?;
    if !src.is_file() {
        return Err(format!("bundled CLI missing at {}", src.display()));
    }

    let (dir, on_path) = cli_install_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let dst = dir.join(exe);
    std::fs::copy(&src, &dst).map_err(|e| format!("could not write {}: {e}", dst.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o755));
    }

    Ok(InstallResult { path: dst.to_string_lossy().into_owned(), on_path })
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

/// Fetches an update and holds it for the exit handler.
///
/// Nothing is asked and nothing restarts. The next time the window is closed the
/// update is applied, and the version after that is the one that opens.
#[tauri::command]
async fn update_download(
    app: AppHandle,
    studio: State<'_, Studio>,
) -> Result<Option<String>, String> {
    use tauri_plugin_updater::UpdaterExt;

    // Already downloaded and waiting — checking again would fetch the same
    // release a second time on every interval.
    if let Some(pending) = studio.pending_update.lock().unwrap().as_ref() {
        return Ok(Some(pending.version.clone()));
    }

    // Every outcome is recorded. The window discards this error — a failed
    // check should not interrupt a conversation — which means without a log,
    // "no update appeared" covers up to date, offline, and a malformed
    // manifest, three situations that look identical and need different fixes.
    let updater = app.updater().map_err(|e| {
        let e = e.to_string();
        bus::log(&format!("update: updater unavailable: {e}"));
        e
    })?;

    let checked = updater.check().await.map_err(|e| {
        let e = e.to_string();
        bus::log(&format!("update: check failed: {e}"));
        e
    })?;

    let Some(update) = checked else {
        bus::log(&format!(
            "update: none available (running {})",
            env!("CARGO_PKG_VERSION")
        ));
        return Ok(None);
    };

    let version = update.version.clone();
    bus::log(&format!(
        "update: {version} available, downloading (running {})",
        env!("CARGO_PKG_VERSION")
    ));

    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| {
            let e = e.to_string();
            bus::log(&format!("update: download of {version} failed: {e}"));
            e
        })?;

    bus::log(&format!(
        "update: {version} downloaded ({} bytes), will install on exit",
        bytes.len()
    ));

    *studio.pending_update.lock().unwrap() = Some(PendingUpdate {
        version: version.clone(),
        update,
        bytes,
    });
    Ok(Some(version))
}

// ---------------------------------------------------------------------------
// Change notification
//
// The window used to poll the log three times a second to find out whether
// anyone had spoken. Polling is how you pay for latency twice: an agent's reply
// sits unread for up to a second and a half, and the app burns a wakeup every
// interval to discover that nothing happened.
//
// The workspace is a directory on this machine, so the OS will simply tell us
// when it changes. The watcher covers the whole workspace, which means the
// shared-file list gets the same treatment for free.
// ---------------------------------------------------------------------------

/// Emitted whenever anything in the workspace changes. The window reloads on it
/// rather than on a timer.
const WORKSPACE_CHANGED: &str = "workspace-changed";

fn watch_workspace(app: AppHandle, dir: PathBuf) -> notify::Result<RecommendedWatcher> {
    use notify::{EventKind, RecursiveMode, Watcher};

    let handle = app.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        // Access alone is not a change. Without this filter, merely reading the
        // log would announce that the log had changed.
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        let _ = handle.emit(WORKSPACE_CHANGED, ());

        // The room changed, so somebody may need waking. Cheap by design:
        // this only decides and enqueues — the turn itself runs on the
        // agent's own thread, so a slow model never holds up the watcher.
        // The Arc is cloned out of the lock first; calling poll while
        // holding Studio would deadlock the moment a turn posted a reply.
        let manager = handle.state::<Studio>().agents.lock().unwrap().clone();
        if let Some(manager) = manager {
            manager.poll();
        }
    })?;

    // The log lives in a subdirectory, so this has to be recursive to see it.
    watcher.watch(&dir, RecursiveMode::Recursive)?;
    Ok(watcher)
}

/// Brings up every agent that can actually run, and returns the manager that
/// wakes them.
///
/// One agent being unavailable must not cost us the others: Codex needs its
/// desktop app and Claude needs a logged-in CLI, and either can be missing on
/// a perfectly good machine. So each is tried, failures are named in the log,
/// and whoever answers gets to work.
fn start_agents() -> Option<std::sync::Arc<manager::AgentManager>> {
    let candidates: Vec<std::sync::Arc<dyn agent::AgentAdapter>> = vec![
        std::sync::Arc::new(claude_adapter::ClaudeAdapter::new()),
        std::sync::Arc::new(codex_adapter::CodexAdapter::new()),
    ];

    let mut ready: Vec<std::sync::Arc<dyn agent::AgentAdapter>> = Vec::new();
    for adapter in candidates {
        match adapter.start() {
            Ok(()) => {
                bus::log(&format!("agent: {} ready", adapter.name()));
                ready.push(adapter);
            }
            // Named, not swallowed. "Nobody answered" and "Codex is not
            // installed" look identical from the room, and only one of them
            // is worth doing anything about.
            Err(e) => bus::log(&format!("agent: {} unavailable: {e}", adapter.name())),
        }
    }

    if ready.is_empty() {
        bus::log("agent: none available — messages will be posted but nobody will be woken");
        return None;
    }
    Some(std::sync::Arc::new(manager::AgentManager::start(ready)))
}

fn main() {
    let workspace = bus::workspace();
    // Before the manager takes its high-water marks, so a migrated room is
    // marked at its end rather than replayed from the top.
    bus::migrate();
    let _ = std::fs::create_dir_all(&workspace);

    // Every run starts with what it is and where, so a log entry can be tied to
    // a version without guessing.
    bus::log(&format!(
        "--- Consortium {} starting, workspace {} ---",
        env!("CARGO_PKG_VERSION"),
        workspace.display()
    ));

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Studio {
            workspace: Mutex::new(workspace.clone()),
            // Held for the life of the app: dropping a watcher stops it, so a
            // watcher that is not kept somewhere silently watches nothing.
            watcher: Mutex::new(None),
            pending_update: Mutex::new(None),
            agents: Mutex::new(None),
        })
        .setup(move |app| {
            match watch_workspace(app.handle().clone(), workspace.clone()) {
                Ok(w) => {
                    *app.state::<Studio>().watcher.lock().unwrap() = Some(w);
                }
                // Not fatal. The window keeps a slow poll as a safety net, so a
                // platform without working notifications is slower rather than
                // broken — but it should say so rather than pretending.
                Err(e) => bus::log(&format!(
                    "workspace watcher unavailable, falling back to polling: {e}"
                )),
            }

            // Off the startup path on purpose. Bringing an agent up means
            // spawning a process and finishing a handshake, and doing that
            // here would hold the window closed for as long as the slowest
            // agent takes to answer — or, if one hangs, forever. The room is
            // perfectly usable before anyone can be woken.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let started = start_agents();
                *handle.state::<Studio>().agents.lock().unwrap() = started;
                // The status line is asking on a timer, but say so now rather
                // than leaving it to look offline until the next tick.
                let _ = handle.emit(WORKSPACE_CHANGED, ());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_workspace,
            set_workspace,
            list_workspace_files,
            reveal_workspace,
            bus_messages_since,
            bus_presence,
            agent_states,
            bus_post,
            bus_clear,
            conversations,
            conversation_active,
            conversation_select,
            conversation_create,
            conversation_attach,
            conversation_set_dir,
            claude_sessions,
            app_version,
            cli_installed,
            install_cli,
            update_check,
            update_download
        ])
        .build(tauri::generate_context!())
        .expect("error while starting Consortium")
        .run(|app, event| {
            // Applied on the way out, where a restart costs nothing. Doing it
            // while the window is open would discard whatever was half-typed in
            // the composer, which is the whole reason this was a manual click
            // before rather than automatic.
            if matches!(event, tauri::RunEvent::Exit) {
                let state = app.state::<Studio>();
                let pending = state.pending_update.lock().unwrap().take();
                if let Some(p) = pending {
                    // Both outcomes, not just the bad one. Logging only
                    // failures meant a successful install left no trace, so
                    // "did it update?" had to be answered by looking at the
                    // timestamp on an exe — inferring something the app knew
                    // for certain and did not say.
                    bus::log(&format!("update: installing {} on exit", p.version));
                    match p.update.install(&p.bytes) {
                        Ok(()) => bus::log(&format!("update: {} installed", p.version)),
                        Err(e) => {
                            bus::log(&format!("could not install update {}: {e}", p.version))
                        }
                    }
                }
            }
        });
}
