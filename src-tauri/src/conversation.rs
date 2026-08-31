// Conversations: one room per piece of work.
//
// Consortium began with a single room, which is fine until you have two
// projects. Then every message about BazMail lands in the same context as every
// message about Consortium, the agents' sessions grow without bound, and
// nothing can be said in one place without being said in the other.
//
// A conversation owns three things, and it is the combination that makes it
// worth having rather than any one of them:
//
//   - its own transcript, so the rooms cannot bleed into each other,
//   - its own session per agent, so an agent woken here remembers what was
//     said here and nothing else,
//   - its own directory, so an agent woken here is already standing in the
//     right repository.
//
// Session ids are derived rather than stored. A conversation's Claude is
// UUIDv5(namespace, "<slug>/claude") — the same id today, after a restart,
// after a reinstall, on a machine where the config was never copied across.
// Storing it would mean a file that can be lost, and losing it silently turns a
// colleague back into a stranger while everything still looks like it works.
// The override exists for the other direction: pointing a conversation at a
// session that already exists elsewhere.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::bus;

/// Namespace for derived session ids. Arbitrary, fixed, and ours: it only has
/// to be stable and unlike anyone else's.
const NAMESPACE: uuid::Uuid = uuid::uuid!("6f9c1a52-3d0e-4b7a-9c21-8e5f4a0d1b33");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// Identity. Used in paths and in derived session ids, so renaming the
    /// display name never costs an agent its memory.
    pub slug: String,
    pub name: String,
    /// Where agents woken here should work. None means the Consortium
    /// workspace — the shared folder, which is the right default for a room
    /// that is not about a particular repository.
    #[serde(default)]
    pub dir: Option<PathBuf>,
    /// Agent name to an existing session id, when this conversation should
    /// continue a conversation that started somewhere else. Empty is normal.
    #[serde(default)]
    pub sessions: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Store {
    active: String,
    conversations: Vec<Conversation>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            active: "general".into(),
            conversations: vec![Conversation {
                slug: "general".into(),
                name: "General".into(),
                dir: None,
                sessions: HashMap::new(),
            }],
        }
    }
}

fn store_path() -> PathBuf {
    let dir = bus::workspace().join(".consortium");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("conversations.json")
}

fn load() -> Store {
    let raw = std::fs::read_to_string(store_path()).unwrap_or_default();
    // A malformed or missing store falls back to the default rather than
    // failing. Losing the list of conversations should cost you the list, not
    // the ability to talk.
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save(store: &Store) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(store_path(), raw).map_err(|e| format!("could not save conversations: {e}"))
}

pub fn list() -> Vec<Conversation> {
    load().conversations
}

pub fn get(slug: &str) -> Option<Conversation> {
    load().conversations.into_iter().find(|c| c.slug == slug)
}

/// Which conversation this process is talking in.
///
/// The environment wins, because that is how a woken agent is told where it is:
/// the adapter sets it when it runs the CLI, so `consortium post` from inside a
/// turn lands in the room the turn came from rather than wherever the window
/// happens to be pointing.
pub fn active() -> String {
    if let Ok(from_env) = std::env::var("CONSORTIUM_CONVERSATION") {
        if !from_env.trim().is_empty() {
            return from_env.trim().to_string();
        }
    }
    load().active
}

pub fn set_active(slug: &str) -> Result<(), String> {
    let mut store = load();
    if !store.conversations.iter().any(|c| c.slug == slug) {
        return Err(format!("there is no conversation called {slug}"));
    }
    store.active = slug.to_string();
    save(&store)
}

/// Adds a conversation, or returns the existing one under that name.
///
/// Idempotent on purpose: creating a conversation that already exists is a
/// thing people do, and it should hand back the room rather than an error or a
/// second room with the same name.
pub fn create(name: &str, dir: Option<PathBuf>) -> Result<Conversation, String> {
    let slug = bus::slug(name);
    if slug.is_empty() {
        return Err("a conversation needs a name".into());
    }

    let mut store = load();
    if let Some(existing) = store.conversations.iter().find(|c| c.slug == slug) {
        return Ok(existing.clone());
    }

    let made = Conversation {
        slug,
        name: name.trim().to_string(),
        dir,
        sessions: HashMap::new(),
    };
    store.conversations.push(made.clone());
    save(&store)?;
    bus::log(&format!("conversation: created {}", made.slug));
    Ok(made)
}

/// Moves a conversation to a different folder, or back to the workspace.
///
/// Sessions are scoped to the directory they were held in, so a room that
/// moves cannot resume what it said in the old place: the derived id will not
/// be found there and a fresh session starts. That is a real cost and the
/// caller should say so, but it is the caller's decision to make — a room
/// created without a folder is otherwise stuck without one forever.
pub fn set_dir(slug: &str, dir: Option<PathBuf>) -> Result<(), String> {
    let mut store = load();
    let Some(conversation) = store.conversations.iter_mut().find(|c| c.slug == slug) else {
        return Err(format!("there is no conversation called {slug}"));
    };

    // An attached session belongs to the old folder and cannot be resumed
    // from the new one. Keeping it would fail on the next wake with 'No
    // conversation found', which is a confusing way to learn this.
    if conversation.dir != dir {
        conversation.sessions.clear();
    }
    conversation.dir = dir.clone();
    save(&store)?;
    bus::log(&format!(
        "conversation: {slug} now works in {}",
        dir.map(|d| d.display().to_string())
            .unwrap_or_else(|| "the shared workspace".into())
    ));
    Ok(())
}

/// Points a conversation's agent at a session that already exists.
///
/// The way to say "this room continues that conversation" — including one
/// started outside Consortium entirely.
pub fn attach_session(slug: &str, agent: &str, session: &str) -> Result<(), String> {
    let mut store = load();
    let Some(conversation) = store.conversations.iter_mut().find(|c| c.slug == slug) else {
        return Err(format!("there is no conversation called {slug}"));
    };
    conversation
        .sessions
        .insert(agent.to_lowercase(), session.to_string());
    save(&store)?;
    bus::log(&format!("conversation: {slug} now uses {session} for {agent}"));
    Ok(())
}

/// The session an agent should resume for this conversation.
///
/// Derived unless the conversation names one. Derivation is what makes an agent
/// the same colleague across restarts without anything being written down.
pub fn session_for(slug: &str, agent: &str) -> String {
    let agent = agent.to_lowercase();
    if let Some(named) = get(slug).and_then(|c| c.sessions.get(&agent).cloned()) {
        return named;
    }
    uuid::Uuid::new_v5(&NAMESPACE, format!("{slug}/{agent}").as_bytes()).to_string()
}

/// Where an agent woken in this conversation should work.
pub fn dir_for(slug: &str) -> PathBuf {
    get(slug)
        .and_then(|c| c.dir)
        .filter(|d| d.is_dir())
        .unwrap_or_else(bus::workspace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversations_session_is_the_same_every_time() {
        // The whole point of deriving rather than storing: this holds after a
        // restart, a reinstall, and on a machine that has never seen the
        // config file.
        let once = session_for("bazmail", "claude");
        assert_eq!(once, session_for("bazmail", "claude"));
        assert_eq!(once, session_for("bazmail", "Claude"), "case is not identity");
        assert!(uuid::Uuid::parse_str(&once).is_ok(), "must satisfy --session-id");
    }

    #[test]
    fn different_rooms_and_agents_never_share_a_session() {
        // If these collided, two projects would share one context, which is the
        // problem conversations exist to solve.
        let a = session_for("bazmail", "claude");
        assert_ne!(a, session_for("consortium", "claude"), "same agent, other room");
        assert_ne!(a, session_for("bazmail", "codex"), "same room, other agent");
    }
}
