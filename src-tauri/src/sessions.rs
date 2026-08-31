// The Claude Code sessions already on this machine.
//
// Consortium derives a session id for each conversation, which is right for a
// room that starts here. It is not right for a room that continues work already
// under way somewhere else — and pasting a UUID is a poor way to ask for that.
//
// Claude Code stores each session as a JSONL transcript under
// ~/.claude/projects/<encoded-directory>/<uuid>.jsonl, and writes a generated
// title into it. That title is what makes a picker possible: "Cross platform
// email client design" is a thing somebody can recognise, and a UUID is not.
//
// The directory matters as much as the title. Sessions are scoped by working
// directory — resuming from elsewhere finds nothing — so a session can only be
// attached to a conversation that runs in the same place. This module reports
// the directory so that rule can be enforced where the choice is made rather
// than discovered as a failed turn later.

use std::path::PathBuf;

use serde::Serialize;

use crate::bus;

/// How much of a transcript to read looking for its title.
///
/// The header is at the top; a session can be megabytes, and reading all of it
/// to find something in the first few lines would make the picker feel broken.
const HEAD_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: String,
    /// What Claude called this conversation, or the folder if it never earned a
    /// title. Short sessions — a one-shot `claude -p` — often have none.
    pub title: String,
    /// Where it was held. A session can only be resumed from here.
    pub dir: String,
    pub age_secs: u64,
}

fn projects_dir() -> Option<PathBuf> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    let dir = PathBuf::from(home).join(".claude").join("projects");
    dir.is_dir().then_some(dir)
}

/// Reads a transcript's header for its title and directory.
fn describe(path: &PathBuf) -> Option<SessionInfo> {
    let id = path.file_stem()?.to_string_lossy().to_string();
    // A transcript is named by its session id, so anything that is not a UUID
    // is some other file that happens to live here.
    if uuid::Uuid::parse_str(&id).is_err() {
        return None;
    }

    let meta = std::fs::metadata(path).ok()?;
    let age_secs = meta
        .modified()
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let head = read_head(path);
    // The hand-rolled reader in bus understands one flat object per line, which
    // is exactly what these are.
    let title = head
        .lines()
        .find_map(|l| bus::field(l, "aiTitle"))
        .filter(|t| !t.trim().is_empty());
    let dir = head
        .lines()
        .find_map(|l| bus::field(l, "cwd"))
        .unwrap_or_default();

    Some(SessionInfo {
        id,
        title: title.unwrap_or_else(|| {
            // No title: name it by where it happened, which is still more use
            // than a UUID.
            match PathBuf::from(&dir).file_name() {
                Some(name) => format!("untitled — {}", name.to_string_lossy()),
                None => "untitled".to_string(),
            }
        }),
        dir,
        age_secs,
    })
}

fn read_head(path: &PathBuf) -> String {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut buf = vec![0u8; HEAD_BYTES];
    let read = f.read(&mut buf).unwrap_or(0);
    buf.truncate(read);
    // Lossy on purpose: a transcript cut mid-character must not cost us the
    // title that sits above the cut.
    String::from_utf8_lossy(&buf).into_owned()
}

/// Every session on this machine, most recently used first.
pub fn list() -> Vec<SessionInfo> {
    let Some(root) = projects_dir() else {
        return Vec::new();
    };

    let mut found = Vec::new();
    let Ok(projects) = std::fs::read_dir(&root) else {
        return found;
    };

    for project in projects.flatten() {
        let Ok(entries) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                if let Some(info) = describe(&path) {
                    found.push(info);
                }
            }
        }
    }

    found.sort_by_key(|s| s.age_secs);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transcript_is_described_by_its_header() {
        let dir = std::env::temp_dir().join("consortium-session-test");
        let _ = std::fs::create_dir_all(&dir);
        let id = "8f14e45f-ceea-467a-9f3a-1b2c3d4e5f60";
        let path = dir.join(format!("{id}.jsonl"));
        std::fs::write(
            &path,
            "{\"type\":\"ai-title\",\"aiTitle\":\"Cross platform email client\"}\n\
             {\"type\":\"user\",\"cwd\":\"C:\\\\Code\\\\BazMail\"}\n",
        )
        .unwrap();

        let info = describe(&path).expect("should describe a well-formed transcript");
        assert_eq!(info.id, id);
        assert_eq!(info.title, "Cross platform email client");
        assert_eq!(info.dir, "C:\\Code\\BazMail");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_untitled_session_is_named_by_where_it_happened() {
        // Short one-shot runs never earn a title, and "untitled" alone would
        // make several of them indistinguishable in a picker.
        let dir = std::env::temp_dir().join("consortium-session-test");
        let _ = std::fs::create_dir_all(&dir);
        let id = "3c6e0b8a-9c15-4f8b-a0c1-d2e3f4a5b6c7";
        let path = dir.join(format!("{id}.jsonl"));
        std::fs::write(&path, "{\"type\":\"user\",\"cwd\":\"C:\\\\Code\\\\Consortium\"}\n").unwrap();

        let info = describe(&path).expect("should still describe it");
        assert_eq!(info.title, "untitled — Consortium");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn files_that_are_not_transcripts_are_ignored() {
        let dir = std::env::temp_dir().join("consortium-session-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("notes.jsonl");
        std::fs::write(&path, "{}\n").unwrap();
        assert!(describe(&path).is_none(), "only UUID-named files are sessions");
        let _ = std::fs::remove_file(&path);
    }
}
