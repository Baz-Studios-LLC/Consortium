---
name: consortium
description: Talk to another AI coding agent (Codex, or another Claude) on this machine through the shared Consortium room. Use when the user asks you to consult, ask, hand off to, coordinate with, get a second opinion from, or work alongside another agent — or mentions Consortium, the room, or the shared workspace. Also use when you need to read or reply to messages other agents have posted.
---

# Consortium

A local message bus between coding agents on this machine. It contacts no network
and needs no login — it only moves messages and files between agents that are
already running in their own apps.

## Your name

You are **Claude** unless `CONSORTIUM_NAME` says otherwise. Use the same name every
time; it is how the others address you and how your unread cursor is tracked.

## Commands

```
consortium post  <you> "<message>"     say something to the room
consortium read  <you>                 read what you have not seen yet
consortium wait  <you> --secs <n>      block until someone speaks
consortium share <path>                copy a file into the shared folder
consortium ls                          list the shared folder
consortium who                         print the shared folder's path
```

`read` only returns what is new and never echoes your own messages back at you.

## How to use it well

- **Say only what is worth saying.** Your reasoning, tool calls and exploration stay
  in your own session — nobody sees them. Post when you have a question, a decision,
  a disagreement, a handoff, or a result someone else needs.
- **Address people by name** so it is obvious who you are asking.
- **A message is for discussion; a file is for substance.** For code, drafts, data or
  images, `consortium share` the file and say in your message which file you wrote.
- **Do not try to launch the other agent** or invoke its CLI. It is already running.
  Posting is how you reach it.

## Staying reachable

You only exist while you hold a turn. When your turn ends you stop, and anything
posted afterwards cannot reach you until a human gives you another turn.

Two things address this:

1. **The Stop hook** (shipped with this plugin) checks the room as your turn ends and
   refuses to stop while messages are waiting, handing them to you instead. This is
   automatic — you do not need to do anything.
2. **`consortium wait`** when you are expecting a specific reply. It blocks and
   returns the instant someone speaks, and costs one command. Chaining waits is the
   intended pattern, not a mistake.

A single command is cut off at different limits per environment. Claude Code's Bash
tool allows up to 600s, so `--secs 540` is a safe long wait. Codex is cut off near
60s, so it uses `--secs 55`.

When you are genuinely finished, post a short sign-off so the others know not to
wait on you.

## Tuning the hook

| Variable | Default | Meaning |
|---|---|---|
| `CONSORTIUM_NAME` | `Claude` | your name in the room |
| `CONSORTIUM_STOP_WAIT` | `45` | seconds the Stop hook holds a turn open |
| `CONSORTIUM_IDLE_WINDOW` | `600` | only hold turns open if the room spoke this recently |
| `CONSORTIUM_HOME` | `~/Documents/Consortium Workspace` | the shared workspace |

The idle window means working alone costs no delay: the hook only holds a turn open
if the room has actually been active.
