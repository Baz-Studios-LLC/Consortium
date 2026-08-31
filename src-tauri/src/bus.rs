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

/// Where diagnostics go so a human can read them.
///
/// Beside the log the room already uses, in the directory the app already owns
/// and already reveals — so "what happened?" needs no rebuild and no new place
/// to look.
pub fn diag_path() -> PathBuf {
    let dir = workspace().join(".consortium");
    let _ = fs::create_dir_all(&dir);
    dir.join("consortium.log")
}

/// Keeps the diagnostic log from growing without bound.
///
/// One rotation, not a scheme: the previous file is kept so a failure is not
/// erased the moment it is followed by chatter, and everything older than that
/// is gone. A log that eats a disk is its own kind of bug.
const DIAG_MAX_BYTES: u64 = 512 * 1024;

/// Records a diagnostic where it can actually be read.
///
/// `eprintln!` alone is invisible in a release build — `windows_subsystem =
/// "windows"` means no console is attached, so every failure this app knew
/// about went nowhere. Both destinations are used: stderr is what you see under
/// `run.cmd`, and the file is what exists in the app people actually install.
///
/// Timestamps are unix seconds. Nothing here parses dates, and a hand-rolled
/// calendar is a strange thing to risk for a log line; ordering and deltas are
/// what a log is read for.
pub fn log(message: &str) {
    eprintln!("{message}");

    let path = diag_path();
    if fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > DIAG_MAX_BYTES {
        let _ = fs::rename(&path, path.with_extension("log.old"));
    }

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        // A failure to write the log is deliberately silent. There is nowhere
        // left to report it, and an app that cannot log should still work.
        let _ = writeln!(f, "{} {}", now_secs(), message);
    }
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

/// Minimal reader for the flat object fields we write. Avoids a serde dependency
/// so this binary stays tiny and quick for an agent to shell out to, but still
/// observes JSON string boundaries and escapes.
pub fn field(line: &str, key: &str) -> Option<String> {
    let mut rest = line.trim_start().strip_prefix('{')?;

    loop {
        rest = rest.trim_start();
        if rest.starts_with('}') {
            return None;
        }

        let (found_key, after_key) = parse_json_string(rest)?;
        rest = after_key.trim_start().strip_prefix(':')?.trim_start();

        if found_key == key {
            if rest.starts_with('"') {
                let (value, _) = parse_json_string(rest)?;
                return Some(value);
            }

            let end = rest
                .char_indices()
                .find(|(_, c)| matches!(c, ',' | '}'))
                .map(|(i, _)| i)
                .unwrap_or(rest.len());
            return Some(rest[..end].trim().to_string());
        }

        rest = skip_json_value(rest)?.trim_start();
        if let Some(after_comma) = rest.strip_prefix(',') {
            rest = after_comma;
            continue;
        }
        if rest.starts_with('}') {
            return None;
        }
        return None;
    }
}

fn skip_json_value(s: &str) -> Option<&str> {
    let s = s.trim_start();
    if s.starts_with('"') {
        let (_, rest) = parse_json_string(s)?;
        return Some(rest);
    }

    let end = s
        .char_indices()
        .find(|(_, c)| matches!(c, ',' | '}'))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    Some(&s[end..])
}

fn parse_json_string(s: &str) -> Option<(String, &str)> {
    if !s.starts_with('"') {
        return None;
    }

    let mut out = String::new();
    let mut i = 1;
    while i < s.len() {
        let c = s[i..].chars().next()?;
        let next = i + c.len_utf8();
        match c {
            '"' => return Some((out, &s[next..])),
            '\\' => {
                let esc = s[next..].chars().next()?;
                let esc_next = next + esc.len_utf8();
                match esc {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let (high, after_high) = read_hex4(s, esc_next)?;
                        if (0xd800..=0xdbff).contains(&high) {
                            if !s[after_high..].starts_with("\\u") {
                                return None;
                            }
                            let (low, after_low) = read_hex4(s, after_high + 2)?;
                            if !(0xdc00..=0xdfff).contains(&low) {
                                return None;
                            }
                            let scalar =
                                0x10000 + (((high - 0xd800) as u32) << 10) + ((low - 0xdc00) as u32);
                            out.push(char::from_u32(scalar)?);
                            i = after_low;
                            continue;
                        }
                        out.push(char::from_u32(high as u32)?);
                        i = after_high;
                        continue;
                    }
                    _ => return None,
                }
                i = esc_next;
            }
            c if (c as u32) < 0x20 => return None,
            c => {
                out.push(c);
                i = next;
            }
        }
    }
    None
}

fn read_hex4(s: &str, start: usize) -> Option<(u16, usize)> {
    let end = start.checked_add(4)?;
    let hex = s.get(start..end)?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some((u16::from_str_radix(hex, 16).ok()?, end))
}

#[cfg(test)]
mod tests {
    use super::{escape, field, mentions};

    #[test]
    fn a_mention_addresses_someone() {
        assert_eq!(mentions("@Codex please review smtp.rs"), vec!["codex"]);
        // Case-insensitive, and mid-sentence is still a mention.
        assert_eq!(mentions("I think @Claude should look"), vec!["claude"]);
    }

    #[test]
    fn an_email_address_is_not_a_mention() {
        // The whole reason mentions must start a word: this used to be the
        // difference between a message to nobody and a message to "example".
        assert!(mentions("write to me@example.com about it").is_empty());
    }

    #[test]
    fn a_statement_addresses_nobody() {
        // Rule six, and the one that stops two agents talking forever: an
        // agent message with no mention wakes no one.
        assert!(mentions("The changes are committed.").is_empty());
    }

    #[test]
    fn several_mentions_are_kept_in_order_without_duplicates() {
        assert_eq!(
            mentions("@Codex and @Claude — @codex you first"),
            vec!["codex", "claude"]
        );
    }

    #[test]
    fn unknown_names_survive_parsing() {
        // The parser does not decide who exists. Dropping an unknown name here
        // would make a typo look exactly like addressing nobody, and the router
        // could never tell the difference or report it.
        assert_eq!(mentions("@Gemini take a look"), vec!["gemini"]);
    }

    #[test]
    fn field_round_trips_control_escapes() {
        let text = "a\u{0001}b\u{0008}c\u{000c}d";
        let line = format!("{{\"from\":\"Alice\",\"text\":\"{}\",\"at\":\"1\"}}", escape(text));

        assert_eq!(field(&line, "text").as_deref(), Some(text));
    }

    #[test]
    fn field_ignores_key_patterns_inside_strings() {
        let from = "sender \"text\":\" fake";
        let text = "actual \"at\":\" still text";
        let line = format!(
            "{{\"from\":\"{}\",\"text\":\"{}\",\"at\":\"1\"}}",
            escape(from),
            escape(text)
        );

        assert_eq!(field(&line, "from").as_deref(), Some(from));
        assert_eq!(field(&line, "text").as_deref(), Some(text));
    }

    #[test]
    fn field_decodes_surrogate_pair_escapes_from_external_json() {
        let line = r#"{"from":"Alice","text":"\ud83e\uddea","at":"1"}"#;

        assert_eq!(field(line, "text").as_deref(), Some("\u{1f9ea}"));
    }
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

/// The names a message explicitly addresses, lowercased and in order.
///
/// Deliberately dumb: it extracts every `@name` without deciding whether that
/// name belongs to a live participant. Knowing who exists is the router's job,
/// and a parser that silently dropped unknown names would make a typo
/// indistinguishable from a message addressed to nobody.
///
/// A mention has to start a word. That is what keeps `mail me@example.com` from
/// reading as a message addressed to "example".
pub fn mentions(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut found: Vec<String> = Vec::new();

    for (i, _) in text.match_indices('@') {
        let starts_word = i == 0
            || bytes
                .get(i - 1)
                .is_some_and(|b| b.is_ascii_whitespace() || matches!(b, b'(' | b'[' | b'{'));
        if !starts_word {
            continue;
        }
        let name: String = text[i + 1..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let name = name.to_lowercase();
        if !found.contains(&name) {
            found.push(name);
        }
    }
    found
}

pub fn post(who: &str, text: &str) {
    // Recipients are stored as a comma-separated string rather than a JSON
    // array. The reader in this file is hand-rolled and understands strings and
    // scalars; teaching it arrays to express a list of two short names is more
    // surface for bugs than the list is worth, and it stays valid JSON either
    // way. Parsed here, at the one boundary both the CLI and the window pass
    // through, so nothing downstream re-reads prose to learn who was addressed.
    let to = mentions(text).join(",");

    let line = format!(
        "{{\"from\":\"{}\",\"text\":\"{}\",\"to\":\"{}\",\"at\":\"{}\"}}\n",
        escape(who),
        escape(text),
        escape(&to),
        now_secs()
    );
    // A failed write must never report success.
    //
    // This used to discard both the open error and the write error and print
    // "posted" regardless, so an agent whose sandbox denied the write believed
    // it was talking to a room that had never heard it — while the others
    // believed it had gone quiet. That cost an hour of two agents waiting on
    // each other. A tool that lies about whether it did the thing is worse than
    // one that cannot do it, because the second kind you can work around.
    let path = log_path();
    let written = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(line.as_bytes()));

    if let Err(e) = written {
        log(&format!(
            "could not post: {e}
  writing to: {}
  nothing was written — the room has not seen this message",
            path.display()
        ));
        std::process::exit(1);
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
            log(&format!("could not share {}: {}", path, e));
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

