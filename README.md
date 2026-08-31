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

### Getting set up

The app bundles the CLI. From a downloaded release:

1. Open Consortium. If the CLI is not installed it says so, next to an **Install CLI**
   button — click it. It copies the bundled binary to the first writable directory
   already on your PATH, and tells you plainly if it could only reach one that is not.
2. Press **Briefing** and paste each agent its instructions.

That is the whole setup. There is nothing to log into.

From a checkout instead:

```bash
npm install
npm run bundle:cli
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
legacy-swift/             the original SwiftUI app, archived
```

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
legacy-swift/             the original SwiftUI app, archived
```

## Notes

- **Nothing is spawned and nothing is authenticated.** Consortium has no HTTP client
  and stores no keys. The agents run in their own apps, already signed in; the bus only
  moves text and files between processes on this machine.
- **PATH.** A Finder-launched `.app` inherits a minimal PATH, so a CLI under
  `/opt/homebrew/bin` is invisible to a naive lookup. `resolve_binary` asks the login
  shell first, then the usual install prefixes. This is only used to tell you whether the
  agents can reach the `consortium` CLI at all.
- **`wait` does not see backlog.** It reports messages posted *after* it starts. Read
  first, then wait — otherwise anything that arrived a moment earlier is skipped.
- **The JSON is hand-rolled** to keep the CLI dependency-free and instant to shell out
  to, since agents call it constantly. `field()` is a small flat-object scanner that
  observes string boundaries and decodes the full escape set including `\uXXXX` and
  surrogate pairs. Earlier versions mangled control characters on read; see
  `docs/bus-field-json-parser.patch` and the tests in `bus.rs`.
- **An agent only exists while it holds a turn.** See *Turns, not daemons* above. This
  is the central constraint of the whole design, not an implementation detail.
