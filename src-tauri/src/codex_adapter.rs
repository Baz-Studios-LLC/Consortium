//! Codex app-server adapter.
//!
//! One long-lived app-server process owns one Consortium-specific Codex thread.
//! The thread id is persisted beside the room log, but the executable path is
//! deliberately rediscovered on every start because Codex Desktop installs it
//! below a content-hashed directory that changes on update.

use crate::agent::{AgentAdapter, AgentState, ContextLine, WakeRequest};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;
use std::time::Duration;

const SUPPORTED_CODEX_VERSION_PREFIX: &str = "codex-cli 0.150.";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const TURN_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const THREAD_ID_FILE: &str = "codex-thread-id";

const DEDICATED_THREAD_INSTRUCTIONS: &str = r#"You are Codex in a shared local room called Consortium.
You are running in a dedicated Consortium-owned thread, never an interactive user's thread.
The user message contains the triggering room message and bounded context. Act on it directly.
Return only the text worth posting back to the room. Do not run `consortium post` yourself.
Use @Name only when that person must answer or do work. Never @-mention merely to thank,
acknowledge, confirm, or sign off. If no room reply is useful, return an empty response."#;

/// A synchronous facade over Codex's newline-delimited app-server protocol.
///
/// The manager serialises calls to `wake`; the mutex also makes that guarantee
/// explicit at the process boundary and keeps state transitions atomic.
pub struct CodexAdapter {
    inner: Mutex<Inner>,
}

struct Inner {
    state: AgentState,
    binary: Option<PathBuf>,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout: Option<Receiver<Result<String, String>>>,
    next_id: u64,
    thread_id: Option<String>,
    thread_workspace: Option<PathBuf>,
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexAdapter {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                state: AgentState::Offline,
                binary: None,
                child: None,
                stdin: None,
                stdout: None,
                next_id: 1,
                thread_id: None,
                thread_workspace: None,
            }),
        }
    }
}

impl AgentAdapter for CodexAdapter {
    fn name(&self) -> &str {
        "codex"
    }

    fn start(&self) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Codex adapter lock poisoned".to_string())?;
        if matches!(inner.state, AgentState::Idle | AgentState::Working) {
            return Ok(());
        }

        inner.state = AgentState::Starting;
        match inner.start_process() {
            Ok(()) => {
                inner.state = AgentState::Idle;
                Ok(())
            }
            Err(error) => {
                inner.terminate();
                inner.state = AgentState::Error(error.clone());
                Err(error)
            }
        }
    }

    fn stop(&self) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Codex adapter lock poisoned".to_string())?;
        inner.terminate();
        inner.state = AgentState::Offline;
        Ok(())
    }

    fn wake(&self, request: &WakeRequest) -> Result<Option<String>, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Codex adapter lock poisoned".to_string())?;
        if inner.state != AgentState::Idle {
            return Err(format!("Codex adapter is {}, not idle", inner.state));
        }

        inner.state = AgentState::Working;
        let result = inner.run_turn(request);
        match &result {
            Ok(_) => inner.state = AgentState::Idle,
            Err(error) => inner.state = AgentState::Error(error.clone()),
        }
        result
    }

    fn state(&self) -> AgentState {
        self.inner
            .lock()
            .map(|inner| inner.state.clone())
            .unwrap_or_else(|_| AgentState::Error("Codex adapter lock poisoned".into()))
    }
}

impl Inner {
    fn start_process(&mut self) -> Result<(), String> {
        let binary = resolve_codex().ok_or_else(|| {
            "Codex executable not found on PATH or in a known Codex Desktop install".to_string()
        })?;
        validate_version(&binary)?;

        let mut child = Command::new(&binary)
            .args(["app-server", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("could not start {}: {e}", binary.display()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Codex app-server exposed no stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Codex app-server exposed no stdout".to_string())?;
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("consortium-codex-stdout".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let item = line.map_err(|e| format!("reading Codex app-server stdout: {e}"));
                    if tx.send(item).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("could not start Codex protocol reader: {e}"))?;

        self.binary = Some(binary);
        self.child = Some(child);
        self.stdin = Some(stdin);
        self.stdout = Some(rx);
        self.next_id = 1;

        let initialized = self.request(
            "initialize",
            json!({
                "clientInfo": { "name": "consortium", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": null
            }),
            REQUEST_TIMEOUT,
        )?;
        let user_agent = initialized
            .get("userAgent")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "Codex initialize response omitted userAgent; protocol is incompatible".to_string()
            })?;
        if !user_agent.contains("0.150.") {
            return Err(format!(
                "unsupported Codex app-server ({user_agent}); Consortium was generated against 0.150.x"
            ));
        }
        Ok(())
    }

    fn terminate(&mut self) {
        self.stdin.take();
        self.stdout.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.binary = None;
        self.thread_id = None;
        self.thread_workspace = None;
    }

    fn run_turn(&mut self, request: &WakeRequest) -> Result<Option<String>, String> {
        let thread_id = self.ensure_thread(request)?;
        let started = self.request(
            "turn/start",
            json!({
                "threadId": thread_id,
                "input": [{
                    "type": "text",
                    "text": format_wake(request),
                    "text_elements": []
                }]
            }),
            REQUEST_TIMEOUT,
        )?;
        let turn_id = started
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or_else(|| "Codex turn/start response omitted turn.id".to_string())?
            .to_string();

        let mut answer: Option<String> = None;
        loop {
            let message = self.recv_value(TURN_TIMEOUT)?;
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let params = message.get("params").unwrap_or(&Value::Null);

            if method == "item/completed"
                && params.get("turnId").and_then(Value::as_str) == Some(turn_id.as_str())
                && params.pointer("/item/type").and_then(Value::as_str) == Some("agentMessage")
            {
                answer = params
                    .pointer("/item/text")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }

            if method == "turn/completed"
                && params.pointer("/turn/id").and_then(Value::as_str) == Some(turn_id.as_str())
            {
                let status = params
                    .pointer("/turn/status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                if status != "completed" {
                    let reason = params
                        .pointer("/turn/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("no error detail");
                    return Err(format!("Codex turn {status}: {reason}"));
                }
                return Ok(answer.and_then(non_empty));
            }
        }
    }

    fn ensure_thread(&mut self, request: &WakeRequest) -> Result<String, String> {
        let workspace = PathBuf::from(&request.workspace);
        if self.thread_workspace.as_ref() != Some(&workspace) {
            self.thread_id = read_thread_id(&workspace);
            self.thread_workspace = Some(workspace.clone());
        }
        if let Some(id) = self.thread_id.clone() {
            match self.request("thread/resume", json!({ "threadId": id }), REQUEST_TIMEOUT) {
                Ok(result) => {
                    let resumed = result
                        .pointer("/thread/id")
                        .and_then(Value::as_str)
                        .unwrap_or(&id);
                    self.thread_id = Some(resumed.to_string());
                    return Ok(resumed.to_string());
                }
                Err(_) => {
                    // A deleted or incompatible persisted thread is recoverable:
                    // create a fresh dedicated one and replace the stale id.
                    self.thread_id = None;
                }
            }
        }

        let created = self.request(
            "thread/start",
            json!({
                "cwd": request.workspace,
                "developerInstructions": DEDICATED_THREAD_INSTRUCTIONS,
                "sandbox": "workspace-write",
                "ephemeral": false
            }),
            REQUEST_TIMEOUT,
        )?;
        let id = created
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or_else(|| "Codex thread/start response omitted thread.id".to_string())?
            .to_string();
        persist_thread_id(&workspace, &id)?;
        self.thread_id = Some(id.clone());
        Ok(id)
    }

    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let line = serde_json::to_string(&json!({ "id": id, "method": method, "params": params }))
            .map_err(|e| format!("encoding Codex {method} request: {e}"))?;
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| "Codex app-server is not running".to_string())?;
        writeln!(stdin, "{line}")
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("writing Codex {method} request: {e}"))?;

        loop {
            let message = self.recv_value(timeout)?;
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(format_protocol_error(method, error));
            }
            return message
                .get("result")
                .cloned()
                .ok_or_else(|| format!("Codex {method} response omitted result"));
        }
    }

    fn recv_value(&self, timeout: Duration) -> Result<Value, String> {
        let rx = self
            .stdout
            .as_ref()
            .ok_or_else(|| "Codex app-server is not running".to_string())?;
        let line = rx
            .recv_timeout(timeout)
            .map_err(|e| format!("waiting for Codex app-server: {e}"))??;
        serde_json::from_str(&line)
            .map_err(|e| format!("invalid JSON from Codex app-server: {e}; line: {line}"))
    }
}

fn format_protocol_error(method: &str, error: &Value) -> String {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("unknown protocol error");
    let code = error
        .get("code")
        .map(Value::to_string)
        .unwrap_or_else(|| "unknown".into());
    format!("Codex {method} failed (code {code}): {message}")
}

fn non_empty(text: String) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn format_wake(request: &WakeRequest) -> String {
    let context = request
        .context
        .iter()
        .map(format_context)
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Consortium wake for {agent} (message #{index}, hop {hops}).\nShared workspace: {workspace}\n\nRecent context (oldest first):\n{context}\n\nTrigger from {sender}:\n{body}",
        agent = request.agent,
        index = request.message_index,
        hops = request.hops,
        workspace = request.workspace,
        sender = request.sender,
        body = request.body
    )
}

fn format_context(line: &ContextLine) -> String {
    format!("{}: {}", line.from, line.text)
}

fn thread_id_path(workspace: &Path) -> PathBuf {
    workspace.join(".consortium").join(THREAD_ID_FILE)
}

fn read_thread_id(workspace: &Path) -> Option<String> {
    fs::read_to_string(thread_id_path(workspace))
        .ok()
        .and_then(non_empty)
}

fn persist_thread_id(workspace: &Path, id: &str) -> Result<(), String> {
    let path = thread_id_path(workspace);
    let parent = path
        .parent()
        .ok_or_else(|| format!("invalid Codex thread id path: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    fs::write(&path, format!("{id}\n"))
        .map_err(|e| format!("could not persist {}: {e}", path.display()))
}

fn validate_version(binary: &Path) -> Result<(), String> {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .map_err(|e| format!("could not query {} version: {e}", binary.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} --version exited with {}",
            binary.display(),
            output.status
        ));
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !version.starts_with(SUPPORTED_CODEX_VERSION_PREFIX) {
        return Err(format!(
            "unsupported Codex version '{version}'; Consortium's app-server adapter was generated against {}x",
            SUPPORTED_CODEX_VERSION_PREFIX
        ));
    }
    Ok(())
}

fn resolve_codex() -> Option<PathBuf> {
    path_lookup("codex")
        .or_else(desktop_install_lookup)
        .or_else(common_paths_lookup)
}

#[cfg(windows)]
fn path_lookup(binary: &str) -> Option<PathBuf> {
    let output = Command::new("where").arg(binary).output().ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .map(PathBuf::from)
            .find(|path| path.is_file())
    })?
}

#[cfg(not(windows))]
fn path_lookup(binary: &str) -> Option<PathBuf> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let output = Command::new(shell)
        .args(["-lc", &format!("command -v {binary}")])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
    path.is_file().then_some(path)
}

#[cfg(windows)]
fn desktop_install_lookup() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var("LOCALAPPDATA").ok()?)
        .join("OpenAI")
        .join("Codex")
        .join("bin");
    fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join("codex.exe"))
        .filter(|path| path.is_file())
        .max_by_key(|path| fs::metadata(path).and_then(|m| m.modified()).ok())
}

#[cfg(not(windows))]
fn desktop_install_lookup() -> Option<PathBuf> {
    None
}

fn common_paths_lookup() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    [".local/bin/codex", ".cargo/bin/codex"]
        .iter()
        .map(|suffix| PathBuf::from(&home).join(suffix))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> WakeRequest {
        WakeRequest {
            agent: "codex".into(),
            message_index: 12,
            sender: "Claude".into(),
            body: "@Codex inspect the adapter".into(),
            context: vec![ContextLine {
                from: "Brett".into(),
                text: "Keep it event-driven".into(),
            }],
            hops: 1,
            workspace: "C:\\room".into(),
        }
    }

    #[test]
    fn wake_prompt_carries_identity_context_and_hops() {
        let prompt = format_wake(&request());
        assert!(prompt.contains("message #12, hop 1"));
        assert!(prompt.contains("Brett: Keep it event-driven"));
        assert!(prompt.contains("Trigger from Claude"));
        assert!(prompt.contains("@Codex inspect the adapter"));
    }

    #[test]
    fn silence_stays_silent() {
        assert_eq!(non_empty(" \n ".into()), None);
        assert_eq!(non_empty(" useful \n".into()), Some("useful".into()));
    }

    #[test]
    fn protocol_errors_are_not_silence() {
        let error = json!({ "code": -32602, "message": "bad params" });
        assert_eq!(
            format_protocol_error("turn/start", &error),
            "Codex turn/start failed (code -32602): bad params"
        );
    }

    #[test]
    fn adapter_name_matches_mentions() {
        assert_eq!(CodexAdapter::new().name(), "codex");
    }

    /// Local smoke test only: CI is not expected to have Codex installed or
    /// authenticated. Starting performs the real version check and initialize
    /// handshake, but deliberately does not invoke the model.
    #[test]
    #[ignore = "requires an installed Codex Desktop app"]
    fn installed_app_server_starts_without_a_turn() {
        let adapter = CodexAdapter::new();
        adapter.start().unwrap();
        assert_eq!(adapter.state(), AgentState::Idle);
        adapter.stop().unwrap();
        assert_eq!(adapter.state(), AgentState::Offline);
    }
}
