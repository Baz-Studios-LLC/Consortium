// The threads each agent already has on this machine.
//
// Consortium derives a session id for a room, which is right for work that
// starts here. It is not right for a room that continues something already
// under way in Claude Code or Codex — and pasting a UUID is a poor way to ask
// for that.
//
// Both tools keep their history as JSONL transcripts recording the directory a
// thread was held in. That directory matters: a thread only resumes from the
// folder it belongs to, so the thread is what decides where an agent works.
//
// Titles come from different places, and both took finding out:
//
//   Claude  ~/.claude/projects/<encoded-dir>/<uuid>.jsonl
//           "aiTitle" is written into the transcript and *rewritten* as the
//           session goes on — one 29MB session carries 381 of them. The last
//           is the current one, and in a long session the first can sit well
//           past any sensible read of the opening, so this reads the end.
//
//   Codex   ~/.codex/sessions/<y>/<m>/<d>/rollout-<stamp>-<uuid>.jsonl
//           carries no title at all. They live in ~/.codex/session_index.jsonl
//           as {id, thread_name}. Without it the only name available is the
//           working directory, and Codex names its scratch folders after the
//           first word of the prompt — so a list of threads came out as "giv",
//           "how", "can", which is worse than useless.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Enough of the opening to carry the session header.
const HEAD_BYTES: u64 = 64 * 1024;
/// Enough of the end to carry the most recent title.
const TAIL_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct Thread {
    pub id: String,
    /// Something a person can recognise in a list.
    pub title: String,
    /// Where it was held, and so where the agent will work if this is chosen.
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

fn read_at(path: &Path, from_start: bool, want: u64) -> String {
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);

    let start = if from_start { 0 } else { len.saturating_sub(want) };
    if f.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }

    let mut buf = vec![0u8; want.min(len.saturating_sub(start)) as usize];
    if f.read_exact(&mut buf).is_err() {
        return String::new();
    }

    let text = String::from_utf8_lossy(&buf).into_owned();
    // Reading from the end lands mid-line; that fragment is not valid JSON and
    // would be dropped anyway, but skipping it keeps the intent clear.
    if from_start || start == 0 {
        text
    } else {
        text.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or_default()
    }
}

/// Every value for `key` anywhere in a JSONL chunk, in order.
///
/// Parsed rather than pattern-matched: Codex nests its metadata under
/// "payload", and these files carry prompts that discuss keys like cwd in
/// prose, which a text scan would happily return.
fn find_all(chunk: &str, key: &str) -> Vec<String> {
    fn walk(v: &serde_json::Value, key: &str, out: &mut Vec<String>) {
        match v {
            serde_json::Value::Object(map) => {
                if let Some(hit) = map.get(key).and_then(|v| v.as_str()) {
                    if !hit.trim().is_empty() {
                        out.push(hit.to_string());
                    }
                }
                for v in map.values() {
                    walk(v, key, out);
                }
            }
            serde_json::Value::Array(items) => {
                for v in items {
                    walk(v, key, out);
                }
            }
            _ => {}
        }
    }

    let mut out = Vec::new();
    for line in chunk.lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            walk(&v, key, &mut out);
        }
    }
    out
}

fn find(chunk: &str, key: &str) -> Option<String> {
    find_all(chunk, key).into_iter().next()
}

fn folder_name(dir: &str) -> String {
    Path::new(dir)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.to_string())
}

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
            // A transcript is named by its session id; anything else in there
            // is some other file that happens to live alongside them.
            uuid::Uuid::parse_str(&id).ok()?;

            let head = read_at(&path, true, HEAD_BYTES);
            let dir = find(&head, "cwd").unwrap_or_default();

            // The last title wins: they are rewritten as the session develops,
            // and an early one describes work that has since moved on.
            let title = find_all(&read_at(&path, false, TAIL_BYTES), "aiTitle")
                .pop()
                .or_else(|| find_all(&head, "aiTitle").pop())
                .unwrap_or_else(|| format!("untitled — {}", folder_name(&dir)));

            Some(Thread {
                id,
                title,
                dir,
                age_secs: age_of(&path),
            })
        })
        .collect()
}

/// Codex's own names for its threads, by id.
fn codex_titles() -> HashMap<String, String> {
    let Some(index) = home().map(|h| h.join(".codex").join("session_index.jsonl")) else {
        return HashMap::new();
    };
    let Ok(raw) = std::fs::read_to_string(index) else {
        return HashMap::new();
    };

    raw.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| {
            Some((
                v.get("id")?.as_str()?.to_string(),
                v.get("thread_name")?.as_str()?.trim().to_string(),
            ))
        })
        .filter(|(_, name)| !name.is_empty())
        .collect()
}

fn codex_threads() -> Vec<Thread> {
    let Some(root) = home().map(|h| h.join(".codex").join("sessions")) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    transcripts(&root, &mut files);
    let titles = codex_titles();

    files
        .into_iter()
        .filter_map(|path| {
            let head = read_at(&path, true, HEAD_BYTES);
            // The id lives in the header rather than the filename: the file is
            // named rollout-<timestamp>-<uuid>, and pulling the uuid back out
            // of that is a parser waiting to be wrong.
            let id = find(&head, "id")?;
            uuid::Uuid::parse_str(&id).ok()?;

            let dir = find(&head, "cwd").unwrap_or_default();
            Some(Thread {
                title: titles
                    .get(&id)
                    .cloned()
                    // Codex names a scratch folder after the first word of the
                    // prompt, so this fallback produces things like "giv". Said
                    // plainly rather than passed off as a name.
                    .unwrap_or_else(|| format!("untitled — {}", folder_name(&dir))),
                id,
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
        // Codex nests under "payload", and the same file carries prompts that
        // mention cwd in prose. Parsing finds the field; scanning would return
        // whichever appeared first.
        let head = "{\"type\":\"message\",\"text\":\"talk about \\\"cwd\\\" here\"}\n\
                    {\"type\":\"session_meta\",\"payload\":{\"id\":\"abc\",\"cwd\":\"C:\\\\Code\\\\BazMail\"}}\n";
        assert_eq!(find(head, "cwd").as_deref(), Some("C:\\Code\\BazMail"));
        assert_eq!(find(head, "id").as_deref(), Some("abc"));
    }

    #[test]
    fn the_most_recent_title_is_the_one_that_counts() {
        // Titles are rewritten as a session develops. Taking the first would
        // describe work it has since moved past.
        let chunk = "{\"aiTitle\":\"early guess\"}\n{\"aiTitle\":\"what it became\"}\n";
        assert_eq!(
            find_all(chunk, "aiTitle").pop().as_deref(),
            Some("what it became")
        );
    }

    #[test]
    fn a_missing_field_is_absent_rather_than_empty() {
        assert_eq!(find("{\"aiTitle\":\"\"}\n", "aiTitle"), None);
        assert_eq!(find("not json at all\n", "aiTitle"), None);
    }

    #[test]
    fn a_thread_without_a_title_is_named_for_its_folder() {
        assert_eq!(folder_name("C:\\Code\\BazMail"), "BazMail");
        assert_eq!(folder_name("/home/b/projects/thing"), "thing");
    }

    #[test]
    fn unknown_agents_have_no_threads_rather_than_failing() {
        assert!(list("gemini").is_empty());
    }
}
