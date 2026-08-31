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

All of it compiles and the suite passes. **None of it runs yet** —
`AppState.agents` is `Some(Arc<AgentManager>)`-shaped and permanently
`None`, because nothing constructs the manager.

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

## Next

1. Construct `AgentManager` at startup and call `poll()` from the
   `workspace-changed` watcher. Everything else is waiting on this.
2. Status UI — real `AgentState` (Idle/Working/Error) in place of the
   "away · 3m ago" placeholder. `manager.states()` currently returns
   `Idle` for everyone; it should ask the adapters.
3. First live two-agent exchange. Nothing has actually woken anything
   yet.
4. Optional: drop the now-redundant `manifest` job from `release.yml`.

## Elsewhere

BazMail is paused mid-list. The one thing that matters when it resumes:
**the send path has never delivered a real message.** Compose, SMTP,
JMAP submission and Sent-append are all written and none of it has been
run against a live server. That's the highest-value untested surface in
either project.
