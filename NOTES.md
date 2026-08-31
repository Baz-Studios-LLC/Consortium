# Where Consortium is

Written 2026-08-31, mid-build. This is the state of the event-driven
wakeup and the reasoning behind it — the parts that would be expensive to
work out a second time.

## The problem this replaces

Agents reached the room by polling: a timer, a CLI call, a model
invocation to discover that usually nothing had happened. A minute of
latency on every message and a bill for asking. Consortium knew a
message had arrived and had no way to say so.

The fix is to push. Consortium owns a process per agent and hands it the
message directly.

## What exists

| File | What it owns | State |
|---|---|---|
| `agent.rs` | `AgentAdapter` trait, `WakeRequest`, `AgentState` | done, committed |
| `router.rs` | who a message wakes, and whether | done, 9 tests |
| `manager.rs` | queueing, dedupe, high-water mark | done, untested at runtime |
| `claude_adapter.rs` | `claude --print --resume` | done, CLI verified |
| `codex_adapter.rs` | Codex app-server protocol | Codex's work, 4 tests |

All of it compiles, the suite passes, and it is **wired**: adapters start on
a background thread at launch, and the filesystem watcher drives
`manager.poll()` on every change to the room.

`ClaudeAdapter` is verified against the live CLI — a real turn goes out and
comes back parsed, with the session id kept (`cargo test -- --ignored`).
Nothing has yet woken anything *through the app*, and no two agents have
exchanged a turn.

## The decisions worth keeping

**Routing is syntax, never prose.** Who a message is for comes from the
sender and the recipients `post` recorded — never from reading the text.
Codex made this argument and it's right: "is this asking for work" is
semantic, and any heuristic for it eventually suppresses a real request
or wakes on politeness.

**A person addressing the room wakes everyone; an agent addressing the
room wakes nobody.** This one rule is most of the loop protection. "The
changes are committed" is a full stop, and treating it as a request is
exactly how two agreeable agents end up talking to each other with
nobody listening.

**Mentioning yourself is not a wake.** Otherwise an agent signing its
own name wakes itself, forever, immediately.

**The hop limit stops agents and never a person.** A limit that could
lock the room against the one participant able to restart it would be
worse than no limit. Hops are counted back through the log rather than
stored in a field, so they can't drift from what was actually said and
they survive a restart with nothing persisted. `MAX_AGENT_HOPS = 8`, and
reaching it posts a line saying so — a limit that fails silently is
indistinguishable from a broken system.

**The high-water mark starts at the end of the log.** A room that
answered a week of backlog the moment it gained the ability to would be
a disaster in its first second.

**Status is asked, never assumed.** `states()` briefly returned `Idle` for
everyone, which meant a crashed agent looked exactly like a healthy one. It
now asks the adapters. A status line that cannot be wrong is not a status
line.

**Failures are announced, never swallowed.** A turn that fails quietly
looks exactly like an agent choosing not to answer, and the room waits
on it forever. This project has already lost an hour to `post` printing
"posted" over a failed write — that's where the rule comes from.

**Silence is a real answer.** `Ok(None)` means nothing worth saying, and
must not become an empty post or an "ok". An acknowledgement is the
thing that wakes somebody else.

**One turn at a time per agent**, enforced by one worker thread and one
channel per adapter rather than a lock somebody could forget to take.

## Verified rather than assumed

- `claude -p --output-format json` returns an **array**; the **last**
  element carries the result. Parsing it as a single object silently
  finds nothing. Confirmed against the real CLI.
- An `is_error` inside a zero exit code is still an error, and must not
  reach the room as though Claude had said it.
- `claude -p` was returning 401 in a clean shell, not a sandbox
  artifact. `claude auth status` said `loggedIn: true` — stored state,
  not a live check, so it lied. `claude auth login` fixed it.

## The folder is the room

Point Consortium at a folder and that is the room: its chat lives in
`<folder>/.consortium/messages.jsonl`, agents woken there work there, and
their memory of it is derived from its path. Point it somewhere else and
that is a different room. Nothing is named, created, or chosen.

This replaced a design with its own list of rooms, each with a name, a
directory and a thread picked per agent. It worked and it asked too much:
three things to set up before anyone could speak, and a picker whose whole
job was to answer a question the folder already answered.

Threads are **derived**: UUIDv5 over `<folder>/<agent>`, lowercased and with
trailing separators trimmed so one folder typed two ways is one room. The
first wake creates the thread, every later wake resumes it. Nothing stored,
so nothing to lose or fall out of step. If an id is ever wrong, the agent
says so — a better error than any check made on its behalf.

What the Claude CLI actually does, established by running it:

| | |
|---|---|
| `--session-id <fresh>` | creates it |
| `--session-id <existing>` | **fails** — "already in use" |
| `--resume <existing>` | continues it |
| `--resume <unknown>` | **fails** — "No conversation found" |

So the first turn creates and the rest resume, decided by trying and reading
the answer. Tracking it in memory would be wrong on the first restart —
exactly when it would still look correct.

**Threads are scoped to their folder.** Resuming from anywhere else finds
nothing. Verified: the same id resolved in one folder and was "not found" in
another. Another reason the folder is the room — the two cannot drift apart
if they are the same thing.

The chosen folder is remembered in `~/.consortium/workspace`, outside any
room, because the CLI is a separate process that has to agree about where it
is. `CONSORTIUM_HOME` overrides it.

## Next

1. **Run the app and say something.** The first real wake through the UI.
   The adapter is verified against the live CLI; the app path is not.
2. First two-agent exchange — needs Codex off its usage limit.
3. Per-agent stop/restart. `AgentAdapter::stop` exists and nothing calls it,
   which currently means a failed agent stays failed until the app restarts.
4. Detaching an attached session — you can point a room at one, not undo it.
5. Optional: drop the now-redundant `manifest` job from `release.yml`.

## Elsewhere

BazMail is paused mid-list. The one thing that matters when it resumes:
**the send path has never delivered a real message.** Compose, SMTP,
JMAP submission and Sent-append are all written and none of it has been
run against a live server. That's the highest-value untested surface in
either project.
