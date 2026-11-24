# 05 — Agents

Status: draft
Owns: agent lifecycle, views, hook events, approvals, agents overview.

## Summary

An agent is a coding-agent CLI (claude-code, codex, …) running in a pane PTY on a worker.
`latticed` captures its hook events into an `events` Log stream. The coding-agent app turns them into a Visual view on top of the live PTY. See [architecture/04-worker.md](../architecture/04-worker.md) › Structured events.

## Lifecycle

1. `+ Agent` (`⌘⇧A`) or `+` menu › `Agent · <name>` starts an agent in a new pane of the current session.
2. The PTY never stops when views change. All views share one event cursor.
3. Agent states: `working`, `waiting` (approval), `idle`, `done`, `error`.
4. Completion raises a toast: "Agent `refactor-auth` finished on build-01 · 12 files" + `Review`.

## Hook events

`latticed` publishes events as a stream (architecture/02-sync.md). Visual mode renders only from these events.

| Event | Renders as |
|---|---|
| UserPromptSubmit | Prompt bubble (`you · web · 10:11:02`) |
| PreToolUse / PostToolUse | Tool group, edit card, run card |
| TodoWrite | Plan checklist (`Plan 3/5`) |
| Notification · permission | Approval card |
| Stop | Final agent text, run summary |

Event count shows in the header (`events → hooks · 214`). TBD: exact event schema per agent; adapter per agent CLI.

## Views

| View | Key | Behavior |
|---|---|---|
| TTY | default | Raw PTY |
| Visual in place | `⌘.` | Same pane renders events |
| Zoom | `⌘⇧↵` | Pane fills center; other panes become live chips; `esc` restores split |
| Mirrored | dock | Pane stays TTY; right panel `Agent` tab shows Visual. Follows focus or pinned |

Visual options: Thinking, Tool output, Inline diffs, Timestamps (toggles); density comfortable / compact.

## Approvals

1. Agent permission request → state `waiting`, glyph bar, `▍1 agent waiting` in status bar.
2. Answer from anywhere: pane, zoom, right panel, agents overview, mobile global card. Keys `Y` Allow, `A` Always, `N` Deny.
3. TTY answers map to the CLI's own prompt (`1. Yes`, `2. Yes, and don't ask again for git push`, `3. No`).
4. "Always" scope = agent + command prefix (`git push`). TBD: where stored, how to revoke.
5. Mobile surfaces approvals from other panes as a sticky card.

## Run panel (full agent session)

Status (`Waiting for approval`), elapsed, tool-call count, context usage %, FILES CHANGED, CHECKPOINTS (`Rewind`), EVENT STREAM.
TBD: checkpoint mechanism (git stash / worktree snapshot?).

## Agents overview

Right panel `Agents` tab (mobile: Tools sheet › Agents). Scope: Session / Worker / All.

| Section | Rows |
|---|---|
| AGENTS | glyph, name, `session · worker`, state + elapsed, current activity, plan progress, inline approval |
| TODOS | grouped per agent; done / now / todo |
| COMMANDS | command, agent, result, duration or `approve` |
| FILES TOUCHED | `A` / `M` / `read`, path, `+a −d` |

## Composer

`Message the agent` · `@file` mentions · options (e.g. `Ask before push`) · `esc` interrupt · `Send ⌘↵`. Attachments and voice: see 07.

## Open questions

- TBD: supported agents list for v0.1 (claude-code, codex confirmed in designs).
- TBD: does a Visual-mode message write into the PTY stdin or a side channel?
