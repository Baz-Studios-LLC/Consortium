// `consortium` — the CLI the agents themselves call.
//
// Claude Code and Codex are already running, already authenticated, in their own
// apps. Neither needs Consortium to launch it. What they lack is a way to reach
// each other, so this binary is a plain local message bus: an append-only log
// plus a shared folder. It never talks to any model or network — it only moves
// text and files between processes on this machine.
//
//   consortium post   <who> <message>   say something to the room
//   consortium read   <who> [--all]     read what you haven't seen yet
//   consortium wait   <who> [--secs N]  block until someone speaks
//   consortium share  <path>            copy a file into the shared folder
//   consortium ls                       list the shared folder
//   consortium who                      show the workspace path

#[path = "../bus.rs"]
mod bus;
// The bus resolves paths through the active conversation, so the CLI needs
// the same notion of where it is talking as the window does.
#[path = "../conversation.rs"]
mod conversation;

use bus::{ls, post, read, share, wait, workspace};

const USAGE: &str = "\
consortium — a local message bus between coding agents

  consortium post  <who> <message>    say something to the room
  consortium read  <who> [--all]      read what you haven't seen yet
  consortium wait  <who> [--secs N]   block until someone speaks (default 120)
  consortium share <path>             copy a file into the shared folder
  consortium ls                       list the shared folder
  consortium who                      print the shared workspace path

<who> is your own name, e.g. \"Claude\" or \"Codex\".
Set CONSORTIUM_HOME to use a workspace other than the default.";

fn main() {
    // Before anything reads or writes: a room that predates conversations
    // has to be moved into one, and the CLI often runs before the window does.
    bus::migrate();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("");

    match cmd {
        "post" if args.len() >= 3 => post(&args[1], &args[2..].join(" ")),
        "read" if args.len() >= 2 => read(&args[1], args.iter().any(|a| a == "--all")),
        "wait" if args.len() >= 2 => {
            let secs = args
                .iter()
                .position(|a| a == "--secs")
                .and_then(|i| args.get(i + 1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(120);
            wait(&args[1], secs)
        }
        "share" if args.len() >= 2 => share(&args[1]),
        "ls" => ls(),
        "who" => println!("{}", workspace().display()),
        _ => {
            println!("{}", USAGE);
            if !cmd.is_empty() && cmd != "help" && cmd != "--help" {
                std::process::exit(1);
            }
        }
    }
}
