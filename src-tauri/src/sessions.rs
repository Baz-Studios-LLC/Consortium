// The threads each agent already has on this machine.
//
// Consortium derives a session id for a room, which is right for work that
// starts here. It is not right for a room that continues something already
// under way in Claude Code or Codex — and pasting a UUID is a poor way to ask
// for that.
//
// Both tools keep their history as JSONL transcripts, and both record the
// directory the thread was held in. That directory is the important part: a
// thread can only be resumed from the folder it belongs to, so the thread is
// what decides where an agent works. Choosing the thread sets the folder, not
// the other way around.
//
//   Claude  ~/.claude/projects/<encoded-dir>/<uuid>.jsonl
//           carries a generated "aiTitle", which is what makes a readable list.
//   Codex   ~/.codex/sessions/<y>/<m>/<d>/rollout-<stamp>-<uuid>.jsonl
//           carries a "session_meta" with id and cwd, and no title — so those
//           are named for their folder and how long ago they ran.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// How much of a transcript to read looking for its header.
///
/// Everything wanted is in the first few lines; these files reach hundreds of
/// megabytes, and reading one to find something at the top would make the
/// picker feel broken.
const HEAD_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct Thread {
    pub id: String,
    /// Something a person can recognise in a list.
    pub title: String,
    /// Where it was held, and therefore where the agent will work if this
    /// thread is chosen.
    pub dir: String,
    pub age_secs: u64,
}

fn home() -> Option<PathBuf> {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(PathBuf::from)
}

fn age_of(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
        .unwrap_or(u64::MAX)
}

fn read_head(path: &Path) -> String {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut buf = vec![0u8; HEAD_BYTES];
    let read = f.read(&mut buf).unwrap_or(0);
    buf.truncate(read);
    // Lossy on purpose: a transcript cut mid-character must not cost us the
    // header above the cut.
    String::from_utf8_lossy(&buf).into_owned()
}

/// Finds the first value for `key` anywhere in a JSONL header.
///
/// Parsed rather than pattern-matched: these headers nest (Codex puts its
/// metadata under "payload"), and a nested key found by scanning text would
/// just as happily match one quoted inside a prompt.
fn find(head: &str, key: &str) -> Option<String> {
    fn walk(v: &serde_json::Value, key: &str) -> Option<String> {
        match v {
            serde_json::Value::Object(map) => {
                if let Some(hit) = map.get(key).and_then(|v| v.as_str()) {
                    if !hit.trim().is_empty() {
                        return Some(hit.to_string());
                    }
                }
                map.values().find_map(|v| walk(v, key))
            }
            serde_json::Value::Array(items) => items.iter().find_map(|v| walk(v, key)),
            _ => None,
        }
    }

    head.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find_map(|v| walk(&v, key))
}

fn folder_name(dir: &str) -> String {
    Path::new(dir)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.to_string())
}

/// Walks a directory tree collecting `.jsonl` files.
fn transcripts(root: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            transcripts(&path, into);
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            into.push(path);
        }
    }
}

fn claude_threads() -> Vec<Thread> {
    let Some(root) = home().map(|h| h.join(".claude").join("projects")) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    transcripts(&root, &mut files);

    files
        .into_iter()
        .filter_map(|path| {
            let id = path.file_stem()?.to_string_lossy().to_string();
            // A transcript is named by its session id, so anything else in
            // there is some other file that happens to live alongside them.
            uuid::Uuid::parse_str(&id).ok()?;

            let head = read_head(&path);
            let dir = find(&head, "cwd").unwrap_or_default();
            Some(Thread {
                id,
                title: find(&head, "aiTitle")
                    .unwrap_or_else(|| format!("untitled — {}", folder_name(&dir))),
                dir,
                age_secs: age_of(&path),
            })
        })
        .collect()
}

fn codex_threads() -> Vec<Thread> {
    let Some(root) = home().map(|h| h.join(".codex").join("sessions")) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    transcripts(&root, &mut files);

    files
        .into_iter()
        .filter_map(|path| {
            let head = read_head(&path);
            // The id lives in the header rather than the filename: the file is
            // named rollout-<timestamp>-<uuid>, and pulling the uuid back out
            // of that is a parser waiting to be wrong.
            let id = find(&head, "id")?;
            uuid::Uuid::parse_str(&id).ok()?;

            let dir = find(&head, "cwd").unwrap_or_default();
            Some(Thread {
                id,
                // Codex records no title, so a thread is named for where it
                // happened. With the time beside it in the list, that is enough
                // to tell two apart.
                title: folder_name(&dir),
                dir,
                age_secs: age_of(&path),
            })
        })
        .collect()
}

/// Threads belonging to one agent, most recently used first.
pub fn list(agent: &str) -> Vec<Thread> {
    let mut found = match agent.to_lowercase().as_str() {
        "claude" => claude_threads(),
        "codex" => codex_threads(),
        _ => Vec::new(),
    };
    found.sort_by_key(|t| t.age_secs);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nested_header_is_read_without_scanning_text() {
        // Codex nests its metadata under "payload", and a prompt in the same
        // file contains the word cwd in prose. Parsing finds the field;
        // scanning would find whichever came first.
        let head = "{\"type\":\"message\",\"text\":\"talk about \\\"cwd\\\" here\"}\n\
                    {\"type\":\"session_meta\",\"payload\":{\"id\":\"abc\",\"cwd\":\"C:\\\\Code\\\\BazMail\"}}\n";
        assert_eq!(find(head, "cwd").as_deref(), Some("C:\\Code\\BazMail"));
        assert_eq!(find(head, "id").as_deref(), Some("abc"));
    }

    #[test]
    fn a_missing_field_is_absent_rather_than_empty() {
        assert_eq!(find("{\"aiTitle\":\"\"}\n", "aiTitle"), None);
        assert_eq!(find("not json at all\n", "aiTitle"), None);
    }

    #[test]
    fn a_thread_without_a_title_is_named_for_its_folder() {
        // Otherwise several untitled threads are indistinguishable in a picker,
        // which is the whole thing this list exists to avoid.
        assert_eq!(folder_name("C:\\Code\\BazMail"), "BazMail");
        assert_eq!(folder_name("/home/b/projects/thing"), "thing");
    }

    #[test]
    fn unknown_agents_have_no_threads_rather_than_failing() {
        assert!(list("gemini").is_empty());
    }
}
