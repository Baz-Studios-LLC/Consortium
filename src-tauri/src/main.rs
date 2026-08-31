// Consortium — an in-house development studio.
//
// Consortium does not talk to any model provider. It drives coding-agent CLIs
// (Claude Code, OpenAI Codex) as background subprocesses, pointed at one shared
// workspace directory, and streams their output back into the UI. The agents own
// their own auth and billing; we own the workspace and the transcript.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

mod bus;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

// ---------------------------------------------------------------------------
// Agent registry
// ---------------------------------------------------------------------------

/// A coding-agent CLI Consortium knows how to drive.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AgentKind {
    ClaudeCode,
    Codex,
}

impl AgentKind {
    fn from_id(id: &str) -> Option<Self> {
        match id {
            "claude" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }

    fn id(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
        }
    }

    /// The executable to look for on disk.
    fn binary(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
        }
    }

    fn install_hint(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "npm install -g @anthropic-ai/claude-code",
            Self::Codex => "install the ChatGPT desktop app, or: npm install -g @openai/codex",
        }
    }

    fn all() -> [AgentKind; 2] {
        [Self::ClaudeCode, Self::Codex]
    }
}

#[derive(Serialize)]
struct AgentInfo {
    id: String,
    name: String,
    installed: bool,
    path: Option<String>,
    version: Option<String>,
    install_hint: String,
}

// ---------------------------------------------------------------------------
// Binary discovery
//
// A Finder-launched .app inherits a minimal PATH — /usr/bin:/bin:/usr/sbin:/sbin —
// so `claude` installed under /opt/homebrew/bin is invisible to a naive lookup.
// We ask the user's login shell for its PATH first, then fall back to the usual
// install locations.
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
    let first = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let p = PathBuf::from(first);
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
    // `where` can return several hits, newline separated — take the first.
    let first = text.lines().next()?.trim().to_string();
    let p = PathBuf::from(first);
    p.is_file().then_some(p)
}

#[cfg(windows)]
fn common_paths_lookup(_binary: &str) -> Option<PathBuf> {
    // `where` already covers PATH, and Windows has no equivalent set of
    // conventional unix install prefixes worth guessing at.
    None
}

#[cfg(not(windows))]
fn common_paths_lookup(binary: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("/opt/homebrew/bin/{}", binary),
        format!("/usr/local/bin/{}", binary),
        format!("{}/.local/bin/{}", home, binary),
        format!("{}/.bun/bin/{}", home, binary),
        format!("{}/.npm-global/bin/{}", home, binary),
        format!("{}/.volta/bin/{}", home, binary),
        format!("/usr/bin/{}", binary),
    ];
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
}

/// Some agents ship inside a desktop app rather than on PATH — Codex is bundled
/// in ChatGPT.app. Checked last so a real PATH install always wins.
#[cfg(target_os = "macos")]
fn bundled_lookup(binary: &str) -> Option<PathBuf> {
    let candidates: &[&str] = match binary {
        "codex" => &["/Applications/ChatGPT.app/Contents/Resources/codex"],
        _ => &[],
    };
    candidates.iter().map(PathBuf::from).find(|p| p.is_file())
}

#[cfg(not(target_os = "macos"))]
fn bundled_lookup(_binary: &str) -> Option<PathBuf> {
    None
}

fn resolve_binary(binary: &str) -> Option<PathBuf> {
    path_lookup(binary)
        .or_else(|| common_paths_lookup(binary))
        .or_else(|| bundled_lookup(binary))
}

fn probe_version(path: &Path) -> Option<String> {
    let out = Command::new(path).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

// ---------------------------------------------------------------------------
// Studio state
// ---------------------------------------------------------------------------

struct Studio {
    workspace: Mutex<PathBuf>,
    /// Agent id -> live pid. The reaper thread owns the `Child` and waits on it;
    /// keeping only the pid here means the entry survives for the whole run, so
    /// `cancel_agent` can still find something to kill.
    running: Mutex<HashMap<String, u32>>,
}

fn default_workspace() -> PathBuf {
    bus::workspace()
}

// ---------------------------------------------------------------------------
// Events streamed to the UI
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
struct AgentEvent {
    agent: String,
    /// "init" | "text" | "tool" | "result" | "stderr" | "error" | "exit"
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost_usd: Option<f64>,
    /// Short end-of-run summary. Kept apart from `text` so the UI can show it on
    /// one line without dumping an agent's entire final message into the footer.
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_error: Option<bool>,
}

impl AgentEvent {
    fn new(agent: &str, kind: &str) -> Self {
        Self {
            agent: agent.to_string(),
            kind: kind.to_string(),
            text: None,
            session_id: None,
            cost_usd: None,
            detail: None,
            is_error: None,
        }
    }

    fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

fn emit(app: &AppHandle, ev: AgentEvent) {
    let _ = app.emit("agent-event", ev);
}

// ---------------------------------------------------------------------------
// stream-json parsing
//
// Claude Code emits one JSON object per line under
// `--output-format stream-json --verbose`. Verified against claude 2.1.193:
//   {"type":"system","subtype":"init","session_id":...,"model":...,"cwd":...}
//   {"type":"assistant","message":{"content":[{"type":"text","text":...}]}}
//   {"type":"result","subtype":"success","is_error":...,"total_cost_usd":...}
// ---------------------------------------------------------------------------

fn parse_claude_line(agent: &str, line: &str) -> Vec<AgentEvent> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        // Not JSON — surface it rather than swallowing it.
        return vec![AgentEvent::new(agent, "stderr").with_text(line)];
    };

    let mut out = Vec::new();
    match v.get("type").and_then(|t| t.as_str()) {
        Some("system") => {
            let mut ev = AgentEvent::new(agent, "init");
            ev.session_id = v.get("session_id").and_then(|s| s.as_str()).map(String::from);
            let model = v.get("model").and_then(|m| m.as_str()).unwrap_or("unknown");
            out.push(ev.with_text(format!("session started · {}", model)));
        }
        Some("assistant") => {
            let session = v.get("session_id").and_then(|s| s.as_str()).map(String::from);
            if let Some(blocks) = v.pointer("/message/content").and_then(|c| c.as_array()) {
                for b in blocks {
                    match b.get("type").and_then(|t| t.as_str()) {
                        Some("text") => {
                            if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                                if !t.trim().is_empty() {
                                    let mut ev = AgentEvent::new(agent, "text").with_text(t);
                                    ev.session_id = session.clone();
                                    out.push(ev);
                                }
                            }
                        }
                        Some("tool_use") => {
                            let name = b.get("name").and_then(|n| n.as_str()).unwrap_or("tool");
                            out.push(AgentEvent::new(agent, "tool").with_text(name));
                        }
                        _ => {}
                    }
                }
            }
        }
        Some("result") => {
            let mut ev = AgentEvent::new(agent, "result");
            ev.session_id = v.get("session_id").and_then(|s| s.as_str()).map(String::from);
            ev.cost_usd = v.get("total_cost_usd").and_then(|c| c.as_f64());
            ev.is_error = v.get("is_error").and_then(|e| e.as_bool());
            ev.detail = ev.cost_usd.map(|c| format!("${:.4}", c));
            if ev.is_error == Some(true) {
                if let Some(t) = v.get("result").and_then(|r| r.as_str()) {
                    ev = ev.with_text(t);
                }
            }
            out.push(ev);
        }
        _ => {}
    }
    out
}

// Codex speaks a different JSONL dialect under `exec --json`. Verified against
// codex-cli 0.151.0-alpha.7.2:
//   {"type":"thread.started","thread_id":...}
//   {"type":"item.completed","item":{"type":"agent_message","text":...}}
//   {"type":"item.completed","item":{"type":"file_change","changes":[{path,kind}]}}
//   {"type":"turn.completed","usage":{"input_tokens":...,"output_tokens":...}}
fn parse_codex_line(agent: &str, line: &str) -> Vec<AgentEvent> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return vec![AgentEvent::new(agent, "stderr").with_text(line)];
    };

    let mut out = Vec::new();
    match v.get("type").and_then(|t| t.as_str()) {
        Some("thread.started") => {
            let mut ev = AgentEvent::new(agent, "init");
            ev.session_id = v.get("thread_id").and_then(|t| t.as_str()).map(String::from);
            out.push(ev.with_text("session started"));
        }
        // Only `item.completed` carries finished content — reacting to
        // `item.started` too would double every message and file change.
        Some("item.completed") => {
            let item = v.get("item");
            let itype = item.and_then(|i| i.get("type")).and_then(|t| t.as_str());
            match itype {
                Some("agent_message") => {
                    if let Some(t) = item.and_then(|i| i.get("text")).and_then(|t| t.as_str()) {
                        if !t.trim().is_empty() {
                            out.push(AgentEvent::new(agent, "text").with_text(t));
                        }
                    }
                }
                Some("file_change") => {
                    if let Some(changes) =
                        item.and_then(|i| i.get("changes")).and_then(|c| c.as_array())
                    {
                        for c in changes {
                            let kind = c.get("kind").and_then(|k| k.as_str()).unwrap_or("edit");
                            let path = c.get("path").and_then(|p| p.as_str()).unwrap_or("");
                            let name = Path::new(path)
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.to_string());
                            out.push(
                                AgentEvent::new(agent, "tool")
                                    .with_text(format!("{} {}", kind, name)),
                            );
                        }
                    }
                }
                Some("command_execution") => {
                    let c = item
                        .and_then(|i| i.get("command"))
                        .and_then(|c| c.as_str())
                        .unwrap_or("command");
                    out.push(AgentEvent::new(agent, "tool").with_text(c));
                }
                _ => {}
            }
        }
        Some("turn.completed") => {
            let mut ev = AgentEvent::new(agent, "result");
            ev.is_error = Some(false);
            // Codex reports tokens, not dollars.
            let input = v.pointer("/usage/input_tokens").and_then(|t| t.as_u64());
            let output = v.pointer("/usage/output_tokens").and_then(|t| t.as_u64());
            ev.detail = match (input, output) {
                (Some(i), Some(o)) => Some(format!("{} in / {} out tokens", i, o)),
                _ => None,
            };
            out.push(ev);
        }
        Some("turn.failed") | Some("error") => {
            let mut ev = AgentEvent::new(agent, "result");
            ev.is_error = Some(true);
            let msg = v
                .pointer("/error/message")
                .and_then(|m| m.as_str())
                .or_else(|| v.get("message").and_then(|m| m.as_str()))
                .unwrap_or("turn failed");
            out.push(ev.with_text(msg));
        }
        _ => {}
    }
    out
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn detect_agents() -> Vec<AgentInfo> {
    AgentKind::all()
        .iter()
        .map(|k| {
            let path = resolve_binary(k.binary());
            AgentInfo {
                id: k.id().into(),
                name: k.display_name().into(),
                installed: path.is_some(),
                version: path.as_deref().and_then(probe_version),
                path: path.map(|p| p.to_string_lossy().into_owned()),
                install_hint: k.install_hint().into(),
            }
        })
        .collect()
}

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

/// The shared exchange folder — whatever either agent writes here, the other can read.
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunRequest {
    agent: String,
    prompt: String,
    /// Continue a prior session so the agent keeps its context across turns.
    session_id: Option<String>,
    /// Wire the *other* agent in over MCP so this one can consult it directly.
    #[serde(default)]
    consult: bool,
}

/// What one agent is told about the other when consulting is on. Kept short on
/// purpose — a long briefing here competes with the user's actual task.
fn consult_briefing(peer: &str, workspace: &Path) -> String {
    format!(
        "You share the working directory {} with {}, another coding agent running \
on this machine. You can consult it directly using its tool, and anything either \
of you writes into that directory is immediately visible to the other. Consult it \
when a second opinion or a genuine division of labour would help — not for routine \
work you can finish yourself.",
        workspace.display(),
        peer
    )
}

#[tauri::command]
fn run_agent(app: AppHandle, req: RunRequest, studio: State<Studio>) -> Result<(), String> {
    let kind = AgentKind::from_id(&req.agent).ok_or("unknown agent")?;
    let agent_id = kind.id().to_string();

    if studio.running.lock().unwrap().contains_key(&agent_id) {
        return Err(format!("{} is already running", kind.display_name()));
    }

    let bin = resolve_binary(kind.binary())
        .ok_or_else(|| format!("{} is not installed — {}", kind.display_name(), kind.install_hint()))?;

    let ws = studio.workspace.lock().unwrap().clone();
    std::fs::create_dir_all(&ws).map_err(|e| e.to_string())?;

    let mut cmd = Command::new(&bin);
    cmd.current_dir(&ws);

    match kind {
        AgentKind::ClaudeCode => {
            cmd.args(["-p", &req.prompt])
                .args(["--output-format", "stream-json"])
                .arg("--verbose")
                // Agents work in the shared folder unattended; without this every
                // edit stalls on a prompt no one can answer from this UI.
                .args(["--permission-mode", "acceptEdits"]);
            if let Some(sid) = &req.session_id {
                cmd.args(["--resume", sid]);
            }
            if req.consult {
                if let Some(codex) = resolve_binary(AgentKind::Codex.binary()) {
                    let cfg = serde_json::json!({
                        "mcpServers": {
                            "codex": { "command": codex.to_string_lossy(), "args": ["mcp-server"] }
                        }
                    });
                    cmd.args(["--mcp-config", &cfg.to_string()])
                        .args(["--append-system-prompt", &consult_briefing("Codex", &ws)]);
                }
            }
        }
        AgentKind::Codex => {
            cmd.arg("exec");
            if req.session_id.is_some() {
                cmd.arg("resume");
            }
            cmd.arg("--json")
                // The shared workspace is not a git repo; Codex refuses to run
                // outside one without this.
                .arg("--skip-git-repo-check")
                // Agents have to be able to write into the shared folder.
                .args(["-s", "workspace-write"])
                .args(["-C", &ws.to_string_lossy()]);
            if req.consult {
                if let Some(claude) = resolve_binary(AgentKind::ClaudeCode.binary()) {
                    cmd.args([
                        "-c",
                        &format!("mcp_servers.claude.command=\"{}\"", claude.to_string_lossy()),
                    ])
                    .args(["-c", "mcp_servers.claude.args=[\"mcp\",\"serve\"]"]);
                }
            }
            // `exec resume` takes SESSION_ID before the prompt.
            if let Some(sid) = &req.session_id {
                cmd.arg(sid);
            }
            let prompt = if req.consult {
                format!("{}\n\n{}", consult_briefing("Claude Code", &ws), req.prompt)
            } else {
                req.prompt.clone()
            };
            cmd.arg(&prompt);
        }
    }

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to start {}: {}", kind.display_name(), e))?;

    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take().ok_or("no stderr")?;

    let pid = child.id();
    studio.running.lock().unwrap().insert(agent_id.clone(), pid);

    // stdout: the structured event stream.
    {
        let app = app.clone();
        let agent_id = agent_id.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.trim().is_empty() {
                    continue;
                }
                let events = match AgentKind::from_id(&agent_id) {
                    Some(AgentKind::Codex) => parse_codex_line(&agent_id, &line),
                    _ => parse_claude_line(&agent_id, &line),
                };
                for ev in events {
                    emit(&app, ev);
                }
            }
        });
    }

    // stderr: keep it visible — a CLI that dies on startup says why here.
    {
        let app = app.clone();
        let agent_id = agent_id.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    emit(&app, AgentEvent::new(&agent_id, "stderr").with_text(line));
                }
            }
        });
    }

    // Reap the child so `running` reflects reality and the UI can re-enable Run.
    {
        let app = app.clone();
        let agent_id = agent_id.clone();
        let mut child = child;
        std::thread::spawn(move || {
            let status = child.wait().ok();
            app.state::<Studio>().running.lock().unwrap().remove(&agent_id);
            let code = status.and_then(|s| s.code()).unwrap_or(-1);
            let mut ev = AgentEvent::new(&agent_id, "exit");
            ev.is_error = Some(code != 0);
            emit(&app, ev.with_text(format!("exited ({})", code)));
        });
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// The message bus
//
// The GUI is just another participant: it reads the same append-only log the
// agents write to with `consortium post`, and posts the user's own messages
// into it. Nothing is spawned and nothing is authenticated.
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
    /// (turn ended — it cannot see anything new until the user gives it a turn).
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
// Thread discovery
//
// Waking an agent is useless without knowing WHICH of its conversations is the
// one sitting in this room. Both hosts leave that on disk — Codex embeds the
// thread id in its rollout filename, Claude Code names the transcript after the
// session id — so we can enumerate candidates and let the user point at the
// right one instead of relying on the agent to self-register.
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ThreadInfo {
    kind: String,     // "codex-thread" | "claude-session"
    id: String,
    label: String,    // first thing the human said, so the row is recognisable
    age_secs: u64,
    project: String,
}

/// First user utterance in a transcript, trimmed to something that fits a row.
fn transcript_label(path: &Path) -> String {
    let Ok(f) = std::fs::File::open(path) else { return String::new() };
    for line in BufReader::new(f).lines().map_while(Result::ok).take(400) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
        // Claude Code: {"type":"user","message":{"content":[{"type":"text","text":...}]}}
        // Codex:       {"type":"response_item","payload":{"content":[{"text":...}]}}
        let text = v.pointer("/message/content/0/text")
            .or_else(|| v.pointer("/payload/content/0/text"))
            .or_else(|| v.pointer("/message/content"))
            .and_then(|t| t.as_str());
        if let Some(t) = text {
            let t = t.trim();
            // Skip the machinery: system preambles and our own wake payloads.
            if t.is_empty() || t.starts_with('<') || t.len() < 4 { continue; }
            let mut out: String = t.chars().take(70).collect();
            if t.chars().count() > 70 { out.push('…'); }
            return out.replace('\n', " ");
        }
    }
    String::new()
}

fn age_of(path: &Path) -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| now.saturating_sub(d.as_secs()))
        .unwrap_or(u64::MAX)
}

fn walk_jsonl(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 5 { return; }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() { walk_jsonl(&p, out, depth + 1); }
        else if p.extension().map(|x| x == "jsonl").unwrap_or(false) { out.push(p); }
    }
}

#[tauri::command]
fn list_threads() -> Vec<ThreadInfo> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut all = Vec::new();

    // Codex: rollout-<timestamp>-<thread-id>.jsonl
    let mut codex = Vec::new();
    walk_jsonl(&PathBuf::from(&home).join(".codex/sessions"), &mut codex, 0);
    for p in codex {
        let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        // The id is the last five dash-separated UUID groups.
        let parts: Vec<&str> = name.split('-').collect();
        if parts.len() < 5 { continue; }
        let id = parts[parts.len() - 5..].join("-");
        all.push(ThreadInfo {
            kind: "codex-thread".into(),
            label: transcript_label(&p),
            age_secs: age_of(&p),
            id,
            project: "Codex".into(),
        });
    }

    // Claude Code: <project-slug>/<session-id>.jsonl
    let mut claude = Vec::new();
    walk_jsonl(&PathBuf::from(&home).join(".claude/projects"), &mut claude, 0);
    for p in claude {
        let id = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let project = p.parent()
            .and_then(|d| d.file_name())
            .map(|s| s.to_string_lossy().trim_start_matches('-').replace('-', "/"))
            .unwrap_or_default();
        let project = project.rsplit('/').next().unwrap_or("").to_string();
        all.push(ThreadInfo {
            kind: "claude-session".into(),
            label: transcript_label(&p),
            age_secs: age_of(&p),
            id,
            project,
        });
    }

    all.sort_by_key(|t| t.age_secs);
    all.truncate(40);
    all
}

#[tauri::command]
fn register_thread(who: String, kind: String, id: String) {
    bus::register(&who, &kind, &id);
}

#[derive(Serialize)]
struct Registered { who: String, kind: String, id: String, age_secs: u64 }

#[tauri::command]
fn thread_registry() -> Vec<Registered> {
    bus::registry().into_iter()
        .map(|(who, kind, id, age_secs)| Registered { who, kind, id, age_secs })
        .collect()
}

// ---------------------------------------------------------------------------
// Self-update
//
// The UI polls `update_check` on a long interval and shows a pill when a newer
// version is published. Installing is always an explicit click — an app that
// restarts itself mid-task would drop a running agent.
// ---------------------------------------------------------------------------

#[tauri::command]
async fn update_check(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(Some(update.version.clone())),
        Ok(None) => Ok(None),
        // A failed check is not worth interrupting the user over — the pill just
        // stays hidden until the next poll.
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

#[tauri::command]
fn cancel_agent(agent: String, studio: State<Studio>) -> Result<(), String> {
    // Look up but don't remove — the reaper thread owns removal, and it also
    // emits the `exit` event the UI needs to re-enable Run.
    let pid = studio.running.lock().unwrap().get(&agent).copied();
    if let Some(pid) = pid {
        #[cfg(windows)]
        let mut c = {
            let mut c = Command::new("taskkill");
            c.args(["/PID", &pid.to_string(), "/T", "/F"]);
            c
        };
        #[cfg(not(windows))]
        let mut c = {
            let mut c = Command::new("kill");
            c.args(["-9", &pid.to_string()]);
            c
        };
        c.status().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() {
    let workspace = default_workspace();
    let _ = std::fs::create_dir_all(&workspace);

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Studio {
            workspace: Mutex::new(workspace),
            running: Mutex::new(HashMap::new()),
        })
        .invoke_handler(tauri::generate_handler![
            detect_agents,
            get_workspace,
            set_workspace,
            list_workspace_files,
            reveal_workspace,
            run_agent,
            cancel_agent,
            update_check,
            update_install,
            bus_messages,
            bus_presence,
            list_threads,
            register_thread,
            thread_registry,
            bus_post,
            cli_installed
        ])
        .run(tauri::generate_context!())
        .expect("error while running Consortium");
}
