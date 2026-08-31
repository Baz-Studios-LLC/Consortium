// Shared by two binaries that each use a different subset of it — the CLI drives
// read/wait/share, the GUI only reads and posts — so unused-here is expected.
#![allow(dead_code)]

// The shared message bus: an append-only JSONL log plus a shared folder, living
// in one workspace directory. Used by both the `consortium` CLI (which the agents
// call) and the studio GUI (which watches and joins in). Deliberately dependency
// free — the CLI is shelled out to constantly and should start instantly.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn workspace() -> PathBuf {
    if let Ok(p) = std::env::var("CONSORTIUM_HOME") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("Documents").join("Consortium Workspace")
}

pub fn log_path() -> PathBuf {
    let dir = workspace().join(".consortium");
    let _ = fs::create_dir_all(&dir);
    dir.join("messages.jsonl")
}

/// Where a given reader got to last time, so `read` only shows what's new.
pub fn cursor_path(who: &str) -> PathBuf {
    let dir = workspace().join(".consortium").join("cursors");
    let _ = fs::create_dir_all(&dir);
    dir.join(format!("{}.cursor", slug(who)))
}

pub fn slug(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect()
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Minimal reader for the two fields we write. Avoids a serde dependency so this
/// binary stays tiny and quick for an agent to shell out to.
pub fn field(line: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\":\"", key);
    let start = line.find(&pat)? + pat.len();
    let bytes: Vec<char> = line[start..].chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            '\\' if i + 1 < bytes.len() => {
                match bytes[i + 1] {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    other => out.push(other),
                }
                i += 2;
            }
            '"' => break,
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Some(out)
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Presence is the only honest answer to "why didn't it reply?". An agent can
/// only act while it holds a live turn, so we record whether it is currently
/// blocked on `wait` (listening) or has finished its turn (away). Nothing here
/// can wake a stopped agent — it just makes the difference visible.
pub fn set_presence(who: &str, state: &str) {
    let dir = workspace().join(".consortium").join("presence");
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(
        dir.join(format!("{}.json", slug(who))),
        format!("{{\"who\":\"{}\",\"state\":\"{}\",\"at\":{}}}", escape(who), state, now_secs()),
    );
}

/// (who, state, seconds-since-update)
pub fn presence() -> Vec<(String, String, u64)> {
    let dir = workspace().join(".consortium").join("presence");
    let Ok(entries) = fs::read_dir(&dir) else { return Vec::new() };
    let now = now_secs();
    entries
        .flatten()
        .filter_map(|e| {
            let body = fs::read_to_string(e.path()).ok()?;
            let who = field(&body, "who")?;
            let state = field(&body, "state")?;
            let at: u64 = body
                .split("\"at\":")
                .nth(1)?
                .trim_matches(|c: char| !c.is_ascii_digit())
                .parse()
                .ok()?;
            Some((who, state, now.saturating_sub(at)))
        })
        .collect()
}

/// A participant registering the handle its host needs in order to reach it
/// again — a Codex thread id, a Claude Code session id. Waking is impossible
/// without this: knowing that "Codex" is away says nothing about WHICH Codex
/// conversation is the one sitting in this room. Only the agent knows that, so
/// only the agent can tell us.
pub fn register(who: &str, kind: &str, id: &str) {
    let dir = workspace().join(".consortium").join("registry");
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(
        dir.join(format!("{}.json", slug(who))),
        format!(
            "{{\"who\":\"{}\",\"kind\":\"{}\",\"id\":\"{}\",\"at\":{}}}",
            escape(who), escape(kind), escape(id), now_secs()
        ),
    );
}

/// (who, kind, id, seconds-since-registered)
pub fn registry() -> Vec<(String, String, String, u64)> {
    let dir = workspace().join(".consortium").join("registry");
    let Ok(entries) = fs::read_dir(&dir) else { return Vec::new() };
    let now = now_secs();
    entries
        .flatten()
        .filter_map(|e| {
            let b = fs::read_to_string(e.path()).ok()?;
            let at: u64 = b.split("\"at\":").nth(1)?
                .trim_matches(|c: char| !c.is_ascii_digit()).parse().ok()?;
            Some((field(&b,"who")?, field(&b,"kind")?, field(&b,"id")?, now.saturating_sub(at)))
        })
        .collect()
}

pub fn read_lines() -> Vec<String> {
    let Ok(f) = File::open(log_path()) else { return Vec::new() };
    BufReader::new(f).lines().map_while(Result::ok).filter(|l| !l.trim().is_empty()).collect()
}

pub fn cursor_of(who: &str) -> usize {
    fs::read_to_string(cursor_path(who))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

pub fn set_cursor(who: &str, n: usize) {
    let _ = fs::write(cursor_path(who), n.to_string());
}

pub fn render(lines: &[String]) -> String {
    let now = now_secs();
    lines
        .iter()
        .filter_map(|l| {
            let age = field(l, "at")
                .and_then(|a| a.parse::<u64>().ok())
                .map(|at| format!(" ({})", ago(now.saturating_sub(at))))
                .unwrap_or_default();
            Some(format!("{}{}: {}", field(l, "from")?, age, field(l, "text")?))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Compact relative age, e.g. "just now", "4m ago".
pub fn ago(secs: u64) -> String {
    match secs {
        0..=44 => "just now".into(),
        45..=5400 => format!("{}m ago", (secs + 30) / 60),
        _ => format!("{}h ago", (secs + 1800) / 3600),
    }
}

pub fn post(who: &str, text: &str) {
    let line = format!(
        "{{\"from\":\"{}\",\"text\":\"{}\",\"at\":\"{}\"}}\n",
        escape(who),
        escape(text),
        now_secs()
    );
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = f.write_all(line.as_bytes());
    }
    // A speaker has by definition seen everything up to its own message.
    set_cursor(who, read_lines().len());
    set_presence(who, "active");
    println!("posted");
}

pub fn read(who: &str, all: bool) {
    let lines = read_lines();
    let from = if all { 0 } else { cursor_of(who) };
    let slice = if from < lines.len() { &lines[from..] } else { &[][..] };
    // Don't echo the reader's own lines back at it.
    let others: Vec<String> = slice
        .iter()
        .filter(|l| field(l, "from").map(|f| f != who).unwrap_or(true))
        .cloned()
        .collect();
    set_cursor(who, lines.len());
    if others.is_empty() {
        println!("(nothing new)");
    } else {
        println!("{}", render(&others));
    }
}

pub fn wait(who: &str, secs: u64) {
    let start = std::time::Instant::now();
    let baseline = read_lines().len();
    set_presence(who, "listening");
    loop {
        let lines = read_lines();
        if lines.len() > baseline {
            let fresh: Vec<String> = lines[baseline..]
                .iter()
                .filter(|l| field(l, "from").map(|f| f != who).unwrap_or(true))
                .cloned()
                .collect();
            if !fresh.is_empty() {
                set_cursor(who, lines.len());
                set_presence(who, "active");
                println!("{}", render(&fresh));
                return;
            }
        }
        if start.elapsed().as_secs() >= secs {
            set_presence(who, "away");
            println!(
                "(nothing after {}s. If you still have work to do, wait again — \
each wait keeps you reachable. If you are done, say so with `post` before you \
stop, so the others know not to expect a reply.)",
                secs
            );
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(700));
    }
}

pub fn share(path: &str) {
    let src = PathBuf::from(path);
    let Some(name) = src.file_name() else {
        eprintln!("not a file: {}", path);
        std::process::exit(1);
    };
    let dst = workspace().join(name);
    match fs::copy(&src, &dst) {
        Ok(_) => println!("shared: {}", dst.display()),
        Err(e) => {
            eprintln!("could not share {}: {}", path, e);
            std::process::exit(1);
        }
    }
}

pub fn ls() {
    let ws = workspace();
    let Ok(entries) = fs::read_dir(&ws) else {
        println!("(workspace is empty)");
        return;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    if names.is_empty() {
        println!("(workspace is empty)");
    } else {
        for n in names {
            println!("{}", n);
        }
    }
}

