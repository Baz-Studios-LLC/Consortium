# Consortium

An in-house development studio. Consortium drives coding-agent CLIs as background
subprocesses against one shared workspace, and streams their output into a single UI.

Consortium never talks to a model provider itself. There is no HTTP client and no API
key storage anywhere in this repo — each agent CLI owns its own auth, transport, and
billing. Consortium owns the workspace and the transcript.

## How it works

Consortium never launches or authenticates an agent. Claude Code and Codex already
run, and are already signed in, inside their own apps. What they lack is a way to
reach each other — so Consortium is a local message bus they both shell out to.

```
Claude Code app  ──┐                        ┌──  Codex / ChatGPT app
                   ├──►  consortium CLI  ◄──┤
   consortium post │      messages.jsonl    │  consortium read
   consortium wait └──►  shared folder  ◄───┘  consortium share
                              ▲
                              │
                    Consortium GUI (you)
```

The bus touches no network and no model, so there is nothing to log into.

## The CLI

```
consortium post  <who> <message>    say something to the room
consortium read  <who> [--all]      read what you have not seen yet
consortium wait  <who> [--secs N]   block until someone speaks
consortium share <path>             copy a file into the shared folder
consortium ls                       list the shared folder
consortium who                      print the shared workspace path
```

Each participant has its own cursor, so `read` only returns what is new and never
echoes a participant's own messages back at it.

Install it where the agents can reach it:

```bash
cargo build --release --bin consortium
cp src-tauri/target/release/consortium /opt/homebrew/bin/
```

Then press **Briefing** in the GUI and paste each agent its instructions. The
modal carries a preset for each known agent plus an **Anyone else** block — type
any name and it writes a briefing for that participant, so the same room works
for a third agent on any host.

Every briefing tells its agent to set up a one-minute heartbeat, because that is
the only thing that brings an agent back after its turn has ended.

## Turns, not daemons

The one real limitation, and it is worth understanding before use: **an agent only
exists while it holds a turn.** When its turn ends it stops, and nothing posted
afterwards reaches it until a human gives it another turn. No heartbeat can wake a
stopped agent, because there is no inbound channel into a finished session.

The workaround is `consortium wait`, which blocks — polling the log internally and
returning the instant anyone speaks. One tool call keeps an agent reachable for its
whole duration, and waits can be chained. The briefing tells both agents to end
every turn with a wait rather than stopping while a question is outstanding.

`wait` also records presence, so the sidebar shows whether an agent is *listening*
(blocked on a wait, will see your message) or *away* (turn over, needs a turn in
its own app). That does not fix the constraint, but it stops it being invisible.

## Running

```bash
npm install
npm run dev      # tauri dev
npm run build    # tauri build
```

## Layout

```
src/index.html            entire frontend (vanilla, no bundler)
src-tauri/src/bus.rs      the message log, shared by both binaries
src-tauri/src/main.rs     the studio GUI
src-tauri/src/bin/        the `consortium` CLI the agents call
plugin/                   optional Claude Code plugin (skill + Stop hook)
legacy-swift/             the original SwiftUI app, archived
```

## The plugin (optional)

`plugin/` is a Claude Code plugin. It is **not required** — the room works with the
CLI alone. It adds two conveniences:

- a **skill** that teaches the protocol, so a session does not need the briefing pasted
- a **Stop hook** that refuses to end a turn while messages are queued, handing them to
  the running session instead

The hook covers the window a heartbeat cannot: a message that arrives while an agent is
*already working*. Without it that message waits for the next heartbeat tick; with it the
turn simply continues. It cannot wake a stopped agent — nothing can.

See `plugin/README.md` to install.

## Notes

- **PATH.** A Finder-launched `.app` inherits a minimal PATH, so `/opt/homebrew/bin/claude`
  is invisible to a naive lookup. `resolve_binary` asks the login shell first, then falls
  back to the usual install locations.
- **Permissions.** Claude Code runs with `--permission-mode acceptEdits`; agents work the
  shared folder unattended and there is no way to answer a permission prompt from this UI.
- **Codex lives inside ChatGPT.app.** It is not on PATH, so `resolve_binary` falls back to
  `/Applications/ChatGPT.app/Contents/Resources/codex`. A real PATH install still wins.
- **The two CLIs speak different dialects.** Claude Code emits `system`/`assistant`/`result`
  under `--output-format stream-json`; Codex emits `thread.started`/`item.completed`/
  `turn.completed` under `exec --json`. Both parsers were written against captured output,
  not documentation. Claude reports cost in dollars, Codex in tokens.
- **Codex needs `--skip-git-repo-check`.** The shared workspace is not a git repo, and Codex
  refuses to run outside one without it. It also runs `-s workspace-write` so it can actually
  write into the shared folder.
