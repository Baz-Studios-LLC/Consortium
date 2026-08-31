// The room is the folder.
//
// An earlier design gave Consortium its own list of rooms, each with a name, a
// directory and a chosen thread per agent. It worked, and it asked too much:
// three things to set up before anyone could speak, and a thread picker whose
// whole job was to answer a question the folder already answers.
//
// A folder holds one chat. Point Consortium at a folder and that is the room.
// Point it somewhere else and that is a different room, with a different chat
// and different memory, and nothing had to be created or named.
//
// Threads are derived rather than chosen. An agent's thread in a folder is
// UUIDv5 over "<folder>/<agent>" — the same id today, after a restart, after a
// reinstall. The first wake creates it and every later wake resumes it, so an
// agent's memory of a folder simply accumulates without anyone managing it.
// Nothing is stored, so nothing can be lost or fall out of step.
//
// If the id turns out to be wrong — pointed at a thread that is not the
// agent's — the agent will say so, which is a better error than any check
// Consortium could make on its behalf.

use crate::bus;

/// Namespace for derived thread ids. Arbitrary, fixed, and ours: it only has to
/// be stable and unlike anyone else's.
const NAMESPACE: uuid::Uuid = uuid::uuid!("6f9c1a52-3d0e-4b7a-9c21-8e5f4a0d1b33");

/// The thread an agent continues in this folder.
pub fn session_for(agent: &str) -> String {
    key_for(&bus::workspace().to_string_lossy(), agent)
}

/// Split out so the derivation can be tested without touching the filesystem.
fn key_for(folder: &str, agent: &str) -> String {
    // Case and trailing separators are presentation, not identity: the same
    // folder typed two ways must not become two different agents.
    let folder = folder.trim_end_matches(['/', '\\']).to_lowercase();
    let agent = agent.to_lowercase();
    uuid::Uuid::new_v5(&NAMESPACE, format!("{folder}/{agent}").as_bytes()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agents_thread_in_a_folder_is_always_the_same() {
        // The point of deriving rather than storing: this holds after a
        // restart, a reinstall, and on a machine that never saw a config file.
        let once = key_for("C:\\Code\\BazMail", "claude");
        assert_eq!(once, key_for("C:\\Code\\BazMail", "claude"));
        assert!(uuid::Uuid::parse_str(&once).is_ok(), "must satisfy --session-id");
    }

    #[test]
    fn the_same_folder_written_differently_is_the_same_room() {
        // A trailing slash or a capital letter is how the path was typed, not
        // which folder it is. Treating them as different would hand an agent a
        // fresh memory for no reason it could explain.
        let plain = key_for("C:\\Code\\BazMail", "claude");
        assert_eq!(plain, key_for("C:\\Code\\BazMail\\", "claude"));
        assert_eq!(plain, key_for("c:\\code\\bazmail", "Claude"));
    }

    #[test]
    fn different_folders_and_agents_never_share_a_thread() {
        // If these collided, two projects would share one memory, which is the
        // whole thing folders-as-rooms exists to prevent.
        let a = key_for("C:\\Code\\BazMail", "claude");
        assert_ne!(a, key_for("C:\\Code\\Consortium", "claude"), "other folder");
        assert_ne!(a, key_for("C:\\Code\\BazMail", "codex"), "other agent");
    }
}
