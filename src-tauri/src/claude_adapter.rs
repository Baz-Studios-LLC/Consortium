// Claude Code, driven headlessly.
//
// `claude --print` runs one turn and exits, which suits a wake exactly: there is
// no process sitting idle between turns and nothing is billed for being
// available. `--resume <session>` is what makes it a conversation rather than a
// series of strangers — the session carries everything said before, so a woken
// Claude knows what it already told the room.
//
// The session id comes back in the result and is kept. Losing it would not
// break anything visibly; it would quietly turn every wake into a first
// meeting, which is worse, because the room would look like it was working.

use std::process::Command;
use std::sync::Mutex;

use crate::agent::{AgentAdapter, AgentState, WakeRequest};
use crate::bus;

pub struct ClaudeAdapter {
    /// Carried between turns so each wake continues the same conversation.
    session: Mutex<Option<String>>,
    state: Mutex<AgentState>,
}

impl Default for ClaudeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeAdapter {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
            state: Mutex::new(AgentState::Offline),
        }
    }

    /// Turns a wake into something worth reading.
    ///
    /// The triggering message and the room around it are given directly. An
    /// agent told only to "check Consortium" would spend a round of tool calls
    /// rediscovering what the sender already knew and could simply have said.
    fn prompt(&self, request: &WakeRequest) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "You have been addressed in Consortium, a shared room, by {}.\n\n",
            request.sender
        ));
        out.push_str(&format!("Shared folder: {}\n\n", request.workspace));

        if !request.context.is_empty() {
            out.push_str("Recent conversation, oldest first:\n\n");
            for line in &request.context {
                out.push_str(&format!("{}: {}\n", line.from, line.text));
            }
            out.push('\n');
        }

        out.push_str(&format!("{}: {}\n\n", request.sender, request.body));
        out.push_str(
            "Do the work if work was asked for. Reply with what belongs in the room and \
             nothing else — your reasoning and tool calls stay in your own session and \
             nobody there can see them.\n\n\
             Mention @Name only when you need that person to act or answer. Do not mention \
             anyone merely to thank, agree, or sign off: every mention wakes someone, and \
             two agents politely acknowledging each other is how a room talks to itself \
             with nobody in it.\n\n\
             If you have nothing worth saying, reply with exactly: (nothing)",
        );
        out
    }
}

impl AgentAdapter for ClaudeAdapter {
    fn name(&self) -> &str {
        "Claude"
    }

    fn start(&self) -> Result<(), String> {
        // Nothing to launch: each turn is its own process. The check is whether
        // the binary can be reached at all, so a missing CLI is reported now
        // rather than as a failed turn later.
        match Command::new("claude").arg("--version").output() {
            Ok(out) if out.status.success() => {
                *self.state.lock().unwrap() = AgentState::Idle;
                Ok(())
            }
            Ok(out) => {
                let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
                *self.state.lock().unwrap() = AgentState::Error(why.clone());
                Err(why)
            }
            Err(e) => {
                let why = format!("claude is not on PATH: {e}");
                *self.state.lock().unwrap() = AgentState::Error(why.clone());
                Err(why)
            }
        }
    }

    fn stop(&self) -> Result<(), String> {
        *self.state.lock().unwrap() = AgentState::Offline;
        Ok(())
    }

    fn wake(&self, request: &WakeRequest) -> Result<Option<String>, String> {
        *self.state.lock().unwrap() = AgentState::Working;

        let mut cmd = Command::new("claude");
        cmd.arg("-p")
            .arg(self.prompt(request))
            .arg("--output-format")
            .arg("json")
            // The shared folder is the one place a woken agent can be expected
            // to touch. Anything wider is a decision for a human to make
            // deliberately, not something an adapter should assume.
            .arg("--add-dir")
            .arg(&request.workspace)
            .current_dir(&request.workspace);

        if let Some(session) = self.session.lock().unwrap().clone() {
            cmd.arg("--resume").arg(session);
        }

        let output = cmd.output().map_err(|e| {
            *self.state.lock().unwrap() = AgentState::Error(e.to_string());
            format!("could not run claude: {e}")
        })?;

        *self.state.lock().unwrap() = AgentState::Idle;
        let raw = String::from_utf8_lossy(&output.stdout);

        // Verified against the real CLI rather than assumed: --output-format
        // json returns an *array* of objects and the last one carries the
        // result. Parsing it as a single object silently finds nothing.
        let parsed: serde_json::Value =
            serde_json::from_str(raw.trim()).map_err(|e| {
                format!("claude returned something that is not JSON ({e}): {}", raw.chars().take(200).collect::<String>())
            })?;

        let result_obj = match &parsed {
            serde_json::Value::Array(items) => items.last().cloned().unwrap_or_default(),
            other => other.clone(),
        };

        // An error reported inside a successful exit is still an error. Letting
        // it through as an ordinary reply would post an API failure into the
        // room as though Claude had said it.
        if result_obj.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
            let why = result_obj
                .get("result")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error")
                .to_string();
            *self.state.lock().unwrap() = AgentState::Error(why.clone());
            return Err(why);
        }

        if let Some(id) = result_obj.get("session_id").and_then(|v| v.as_str()) {
            let mut session = self.session.lock().unwrap();
            if session.as_deref() != Some(id) {
                bus::log(&format!("claude: session {id}"));
                *session = Some(id.to_string());
            }
        }

        let reply = result_obj
            .get("result")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();

        // The prompt offers this exact word for "nothing worth saying", so that
        // declining to speak is a thing the agent can do rather than something
        // it has to fake with an empty string.
        if reply.is_empty() || reply == "(nothing)" {
            return Ok(None);
        }
        Ok(Some(reply))
    }

    fn state(&self) -> AgentState {
        self.state.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::ContextLine;

    fn request(body: &str) -> WakeRequest {
        WakeRequest {
            agent: "claude".into(),
            message_index: 0,
            sender: "Brett".into(),
            body: body.into(),
            context: vec![ContextLine { from: "Brett".into(), text: "earlier line".into() }],
            hops: 0,
            workspace: std::env::temp_dir().to_string_lossy().into_owned(),
        }
    }

    #[test]
    fn the_prompt_hands_over_the_room_rather_than_a_pointer_to_it() {
        // The whole reason to push instead of poll: an agent told only to go
        // and look spends a round of tool calls learning what the sender
        // already knew and could simply have said.
        let p = ClaudeAdapter::new().prompt(&request("please look at the parser"));
        assert!(p.contains("please look at the parser"), "the message itself");
        assert!(p.contains("Brett"), "who is asking");
        assert!(p.contains("earlier line"), "what came before");
        assert!(p.contains("(nothing)"), "a way to decline that is not an empty reply");
    }

    #[test]
    #[ignore = "spends a real Claude turn; run explicitly"]
    fn a_real_turn_comes_back_parsed() {
        // The one thing unit tests cannot fake: --output-format json returns
        // an *array* whose last element carries the result, and reading it as
        // a single object finds nothing while looking perfectly healthy.
        let adapter = ClaudeAdapter::new();
        adapter.start().expect("claude CLI should be reachable and logged in");

        let reply = adapter
            .wake(&request("Reply with exactly the word: pineapple"))
            .expect("the turn should succeed")
            .expect("a reply, not silence");
        assert!(reply.to_lowercase().contains("pineapple"), "got: {reply}");

        // Without the session id every wake is a first meeting, and the room
        // would look like it was working the whole time.
        assert!(adapter.session.lock().unwrap().is_some(), "session not kept");
    }
}
