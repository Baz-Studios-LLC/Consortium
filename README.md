# Consortium

An in-house development studio. Consortium drives coding-agent CLIs as background
subprocesses against one shared workspace, and streams their output into a single UI.

Consortium never talks to a model provider itself. There is no HTTP client and no API
key storage anywhere in this repo — each agent CLI owns its own auth, transport, and
billing. Consortium owns the workspace and the transcript.

## Agents

| Agent | Binary | Install |
|---|---|---|
| Claude Code | `claude` | `npm install -g @anthropic-ai/claude-code` |
| Codex | `codex` | ships inside ChatGPT.app, or `npm install -g @openai/codex` |

Agents are detected at launch. A missing CLI is shown greyed out with its install
command rather than failing at run time.

## Shared workspace

Both agents run with their working directory set to the shared workspace
(`~/Documents/Consortium Workspace` by default). Anything one agent writes there —
source files, images, notes — is on disk for the other to read. That folder is the
handoff mechanism; the sidebar lists its contents and polls for changes.

## Running

```bash
npm install
npm run dev      # tauri dev
npm run build    # tauri build
```

## Layout

```
src/index.html          entire frontend (vanilla, no bundler)
src-tauri/src/main.rs   agent discovery, spawning, stream parsing
legacy-swift/           the original SwiftUI app, archived
```

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
