// What an agent looks like to Consortium.
//
// Consortium has never started an agent. Claude Code and Codex run in their own
// apps and reach the room by shelling out to the CLI, which works but means each
// one has to keep asking whether anything happened — a minute of latency and a
// model invocation to discover, usually, that nothing did.
//
// An adapter is how Consortium gets to push instead. It owns one long-lived
// agent process and translates a wake request into whatever that agent speaks:
// the Codex app-server protocol on one side, `claude --resume --print` on the
// other. Nothing above this file knows which.
//
// Two rules this file exists to enforce by shape rather than by discipline:
//
//   - An adapter never decides *whether* it should have been woken. Routing is
//     the manager's job. An adapter that consulted the room to see if a message
//     was for it would be polling again, wearing a different hat.
//   - An adapter never reports success it did not have. That mistake has
//     already cost this project an hour once, when `post` printed "posted" over
//     a failed write.

use std::fmt;

/// Where an agent is, as far as Consortium can tell.
///
/// Derived from the process and the protocol — never from asking the model.
/// Eliminating model heartbeats is the entire point; replacing them with a
/// heartbeat that says "are you idle?" would give the same bill a new name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    /// No process, by choice or because it was never started.
    Offline,
    /// Process launched, protocol handshake not finished.
    Starting,
    /// Alive and waiting. The resting state, and the cheap one.
    Idle,
    /// Mid-turn. Further wakes queue rather than starting a second turn.
    Working,
    /// Process is up but unusable — a failed handshake, a protocol mismatch.
    /// Distinct from Offline because the fix is different: this one needs
    /// looking at, not starting.
    Error(String),
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentState::Offline => write!(f, "offline"),
            AgentState::Starting => write!(f, "starting"),
            AgentState::Idle => write!(f, "idle"),
            AgentState::Working => write!(f, "working"),
            AgentState::Error(why) => write!(f, "error: {why}"),
        }
    }
}

/// Everything an agent needs to act, handed to it directly.
///
/// Deliberately not "go and check the room". Consortium already knows which
/// message caused this and what came before it, so making the agent rediscover
/// that costs a round of tool calls to learn something the sender could simply
/// have said.
#[derive(Debug, Clone)]
pub struct WakeRequest {
    /// Who is being woken, lowercased — "claude", "codex".
    pub agent: String,
    /// Which room this happened in. An agent's memory, working directory and
    /// transcript all hang off this.
    pub conversation: String,
    /// The session this agent should continue for this conversation.
    ///
    /// Chosen by Consortium rather than by the agent, which is what makes an
    /// agent the same colleague here tomorrow. Adapters whose tool has no
    /// notion of a session are free to ignore it.
    pub session: String,
    /// Index of the triggering message in the log. The log is append-only and
    /// cursors already identify messages this way, so the two agree by
    /// construction rather than by a second scheme that could drift.
    pub message_index: usize,
    pub sender: String,
    pub body: String,
    /// Enough of what came before to make the request make sense, oldest first.
    /// Bounded on purpose: the whole room would grow without limit and most of
    /// it is irrelevant to any single request.
    pub context: Vec<ContextLine>,
    /// How many agent-to-agent wakes have happened since a human last spoke.
    /// Carried so an adapter can say so if it declines, and so the manager's
    /// limit is visible rather than mysterious.
    pub hops: u32,
    /// Where to work: the conversation's directory.
    ///
    /// Also where the session lives. Claude Code scopes sessions by working
    /// directory, so resuming from somewhere else finds nothing — verified,
    /// not assumed. A conversation must therefore always run in the same
    /// place, and moving one costs it its memory.
    pub workspace: String,
    /// The room's shared folder, where agents leave things for each other.
    ///
    /// Separate from `workspace`: an agent continuing a thread held in its own
    /// repository still needs somewhere common to put what it produces.
    pub shared: String,
}

#[derive(Debug, Clone)]
pub struct ContextLine {
    pub from: String,
    pub text: String,
}

/// One agent's process and protocol.
///
/// Implementations own a process and a translation, and nothing else. Recipient
/// routing, queueing, deduplication and hop limits all live in the manager, so
/// two adapters cannot hold different opinions about the rules.
pub trait AgentAdapter: Send + Sync {
    /// Lowercased name this adapter answers to, matching what `@mentions`
    /// produce.
    fn name(&self) -> &str;

    /// Bring the process up. Called at startup or after a crash.
    ///
    /// Must not invoke the model. Starting a process is not a turn, and an
    /// adapter that greeted its model on launch would bill for being available.
    fn start(&self) -> Result<(), String>;

    /// Shut the process down cleanly. Called on exit, and before a restart.
    fn stop(&self) -> Result<(), String>;

    /// Run one turn, returning what the agent wants said in the room.
    ///
    /// Returning `Ok(None)` means the agent had nothing to say, which is a
    /// legitimate outcome and must not be turned into an empty post — silence
    /// is quieter than "ok".
    ///
    /// An error here means the turn did not happen. The manager keeps the
    /// triggering message unhandled rather than marking it done, because a
    /// wake that failed is not a wake that was answered.
    fn wake(&self, request: &WakeRequest) -> Result<Option<String>, String>;

    fn state(&self) -> AgentState;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_states_carry_their_reason() {
        // Offline and Error are different situations with different fixes, and
        // the status line has to be able to say which.
        assert_eq!(AgentState::Idle.to_string(), "idle");
        assert_eq!(
            AgentState::Error("handshake failed".into()).to_string(),
            "error: handshake failed"
        );
        assert_ne!(AgentState::Offline, AgentState::Error("x".into()));
    }
}
