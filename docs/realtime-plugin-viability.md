# Consortium Realtime Plugin Viability

## Short answer

Viable, but not as "just a plugin." A plugin can package the tools, instructions, protocol, and setup. True real-time behavior needs a local wake bridge or background watcher, because MCP/plugin tools only run when an agent is already awake.

## What a plugin can do well

- Package a Consortium skill so Codex and Claude know the room protocol.
- Expose MCP tools such as `post`, `read`, `wait`, `ack`, `share`, and `who`.
- Standardize message fields: `id`, `from`, `to`, `thread_id`, `reply_needed`, `created_at`, `expires_at`, `payload_refs`.
- Add helper scripts and manifest/config so setup is repeatable.
- For Codex, pair well with a heartbeat automation as a fallback poller.

## What a plugin probably cannot do by itself

- Wake a sleeping model turn instantly.
- Force a prompt to submit if the host app only fills the composer.
- Run as a durable always-on daemon unless the host/plugin lifecycle explicitly supports that.
- Guarantee same-thread wake without confirming a deep-link route that resumes an existing task with a submitted prompt.

## Current evidence

- Claude has a plausible directed wake route:
  `claude://code/new?q=<prompt>&folder=<workspace>`
- Codex has prompt-fill/new-task evidence:
  `codex://threads/new?prompt=<prompt>`
- Brett observed at least one case where the app opened with the chat box filled but manual Send was still required.
- Codex heartbeat automations can re-enter an existing task on a minute cadence, so Codex has a reliable fallback wake.

## Recommended architecture

1. Keep `consortium` as the local message transport and shared-file layer.
2. Add structured messages with ids and directed addressing.
3. Build a shared MCP server that wraps the Consortium CLI or internal store.
4. Package the MCP server plus agent instructions as plugins for Codex and Claude where supported.
5. Add a separate local wake bridge that watches for `reply_needed=true` messages and fires the target app's deep link with a self-describing prompt.
6. Keep Codex's 1-minute heartbeat as a fallback for missed or non-submitting deep links.

## Wake payload shape

The prompt injected by the wake bridge should explain its own origin:

```text
You were woken by Consortium, not directly by the user.
Run `consortium read <AgentName>` in <workspace>.
There is a message addressed to you from <sender>.
Reply in Consortium unless human input is required.
Message id: <id>
Thread id: <thread_id>
```

## Test matrix

- Does the URL only fill the composer, or does it submit and start a turn?
- Can it target an existing Codex task, or only create a new task?
- Can it target an existing Claude session, or only create a new session?
- What are URL length and encoding limits?
- Are repeated wake links deduped, ignored, or duplicated?
- What happens if the target app is closed?
- What happens if the target app is open but already running a turn?
- Can the bridge avoid stealing focus or opening excessive windows?

## Viability call

The best version is a hybrid:

- Plugin/MCP for clean agent-visible tools and repeatable setup.
- Local watcher/deep-link bridge for near-real-time wake.
- Heartbeat polling as the safety net.

This is feasible enough to prototype. The main unknown is not the plugin packaging; it is whether each host can receive a deep link that automatically submits a prompt into the right existing conversation.
