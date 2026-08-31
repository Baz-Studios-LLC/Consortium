// Claude Code, driven headlessly.
//
// `claude --print` runs one turn and exits, which suits a wake exactly: there is
// no process sitting idle between turns and nothing is billed for being
// available. The session is what makes it a conversation rather than a series
// of strangers.
//
// Consortium chooses the session id rather than accepting whatever the CLI
// mints, because the id is derived from the conversation and so is the same one
// tomorrow. That turns out to require care, all of it verified against the real
// CLI rather than assumed:
//
//   - `--session-id <fresh>` creates that session.
//   - `--session-id <existing>` fails: "Session ID ... is already in use."
//   - `--resume <existing>` continues it.
//   - `--resume <unknown>` fails: "No conversation found with session ID: ..."
//
// So the first turn in a room creates and every turn after resumes, and the
// adapter works out which by trying to resume and reading the answer. Tracking
// it in memory instead would be wrong the first time Consortium restarts, which
// is exactly when it would look like it was working.
//
// Sessions are also scoped to the working directory: resuming from somewhere
// else finds nothing. A conversation therefore always runs in its own
// directory, and moving one costs it its memory.

use std::process::Command;
use std::sync::Mutex;

use crate::agent::{AgentAdapter, AgentState, WakeRequest};
use crate::bus;

/// What the CLI says when a session id is not one it knows here.
const NO_SUCH_SESSION: &str = "No conversation found with session ID";

pub struct ClaudeAdapter {
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
            "You have been addressed in the {} room of Consortium, a shared space, by {}.\n\n",
            request.conversation, request.sender
        ));
        out.push_str(&format!("Working folder: {}\n\n", request.workspace));

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

    /// One invocation. `resume` continues the room's session; otherwise it is
    /// created under the id Consortium chose for this room.
    fn run(&self, request: &WakeRequest, resume: bool) -> Result<String, String> {
        let mut cmd = Command::new("claude");
        cmd.arg("-p")
            .arg(self.prompt(request))
            .arg("--output-format")
            .arg("json");

        if resume {
            cmd.arg("--resume").arg(&request.session);
        } else {
            cmd.arg("--session-id").arg(&request.session);
        }

        cmd
            // The conversation's own folder is the one place a woken agent can
            // be expected to touch. Anything wider is a decision for a human to
            // make deliberately, not something an adapter should assume.
            .arg("--add-dir")
            .arg(&request.workspace)
            .current_dir(&request.workspace)
            // So that `consortium post` from inside the turn lands back in the
            // room the turn came from, rather than wherever the window happens
            // to be pointing.
            .env("CONSORTIUM_CONVERSATION", &request.conversation);

        let output = cmd
            .output()
            .map_err(|e| format!("could not run claude: {e}"))?;

        // The CLI reports a missing session on stdout and exits cleanly, so
        // neither the exit code nor stderr alone is enough to tell what
        // happened.
        let mut raw = String::from_utf8_lossy(&output.stdout).to_string();
        if raw.trim().is_empty() {
            raw = String::from_utf8_lossy(&output.stderr).to_string();
        }
        Ok(raw)
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

        // Resume first. A room that has been spoken in before is the common
        // case, and the uncommon one announces itself clearly.
        let attempted = match self.run(request, true) {
            Ok(raw) if raw.contains(NO_SUCH_SESSION) => {
                bus::log(&format!(
                    "claude: first turn in {}, creating session {}",
                    request.conversation, request.session
                ));
                self.run(request, false)
            }
            other => other,
        };

        let fail = |why: String| -> String {
            *self.state.lock().unwrap() = AgentState::Error(why.clone());
            why
        };

        let raw = match attempted {
            Ok(raw) => raw,
            Err(e) => return Err(fail(e)),
        };

        // Verified against the real CLI rather than assumed: --output-format
        // json returns an *array* and the last element carries the result.
        // Parsing it as a single object silently finds nothing.
        let parsed: serde_json::Value = match serde_json::from_str(raw.trim()) {
            Ok(v) => v,
            Err(e) => {
                return Err(fail(format!(
                    "claude returned something that is not JSON ({e}): {}",
                    raw.chars().take(200).collect::<String>()
                )))
            }
        };

        let result = match &parsed {
            serde_json::Value::Array(items) => items.last().cloned().unwrap_or_default(),
            other => other.clone(),
        };

        // An error reported inside a successful exit is still an error. Letting
        // it through as an ordinary reply would post an API failure into the
        // room as though Claude had said it.
        if result.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
            return Err(fail(
                result
                    .get("result")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error")
                    .to_string(),
            ));
        }

        *self.state.lock().unwrap() = AgentState::Idle;

        let reply = result
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
            conversation: "test-room".into(),
            // Fixed, so the first run creates it and every later run resumes
            // it — which is the behaviour under test.
            session: "4d7c8e10-2b3a-4f56-9a81-0c5d6e7f8a90".into(),
            message_index: 0,
            sender: "Brett".into(),
            body: body.into(),
            context: vec![ContextLine {
                from: "Brett".into(),
                text: "earlier line".into(),
            }],
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
        assert!(p.contains("test-room"), "which room");
        assert!(
            p.contains("(nothing)"),
            "a way to decline that is not an empty reply"
        );
    }

    #[test]
    #[ignore = "spends a real Claude turn; run explicitly"]
    fn a_real_turn_comes_back_parsed() {
        // Covers what no unit test can fake: the array-shaped output, and the
        // create-then-resume dance. Running it twice exercises both paths —
        // the first creates the session, the second resumes it.
        let adapter = ClaudeAdapter::new();
        adapter
            .start()
            .expect("claude CLI should be reachable and logged in");

        let reply = adapter
            .wake(&request("Reply with exactly the word: pineapple"))
            .expect("the turn should succeed")
            .expect("a reply, not silence");
        assert!(reply.to_lowercase().contains("pineapple"), "got: {reply}");
    }
}
