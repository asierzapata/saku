# wrk — Agent Task Executor for Saku

## What it is

`wrk` is the execution layer of the saku productivity suite. Where `tdo` defines and tracks work, `wrk` does it. It watches for agent-assigned tasks, executes them through Claude Code, and reports results back to tdo.

It is **not** an AI assistant platform. It doesn't manage conversations, integrations, or plugins. It does one thing: pick up structured tasks and execute them.

## Who it's for

Developers and makers who already use tdo to manage their work and want their agent to operate within the same system — not alongside it. The kind of person who prefers a task spec over a chat message.

## Core concepts

- **Task as prompt.** A tdo task (title + notes) is the execution spec. No separate prompt files, no chat threads. The task *is* the instruction.
- **Claim before execute.** When wrk picks up a task, it marks it `in_progress` with `assign = wrk` before running. This prevents double-pickup across devices.
- **Report back in-band.** Results go back as tdo task updates — completion notes, status changes. The user checks tdo, not a separate dashboard.
- **tdo is the authority.** wrk never creates its own state. It reads from tdo, writes to tdo. If wrk crashes, tdo still has the truth.

## Commands

### `wrk run <task-number>`

Execute a single task immediately. Reads the task from tdo, runs Claude Code with the task notes as prompt, marks done on success.

```
wrk run 358
wrk run 358 --dry          # show what would be sent to claude, don't execute
wrk run 358 --review       # on success, mark as "needs review" instead of "done"
```

### `wrk daemon`

Long-running process that polls tdo for agent-assigned tasks and executes them.

```
wrk daemon                     # poll with default interval
wrk daemon --poll 30s          # custom poll interval
wrk daemon --once              # single poll cycle, then exit (useful for cron)
wrk daemon --max-concurrent 2  # run up to N tasks in parallel
```

Daemon behavior:
1. Poll tdo for tasks where `assign = agent` and status is pending/ready
2. Respect dependency order — don't pick up blocked tasks
3. Claim task (mark in_progress)
4. Spawn `claude --print` with task context
5. On success: `tdo done <id> --note "summary of what was done"`
6. On failure: add error note to task, leave as in_progress
7. Sync results (if sync configured)

### `wrk status`

Show what wrk is currently doing or last did.

```
wrk status                 # current/recent execution state
wrk status --history       # last N executions with outcomes
```

### `wrk log <task-number>`

View the full execution log for a task run.

```
wrk log 358                # stdout/stderr from the claude execution
```

## Prompt construction

When wrk executes a task, it assembles the prompt from tdo data:

```
[tdo context output — project, area, today summary]
[task title]
[task notes — the detailed spec]
[subtask list, if any]
[blocker status]
[project CLAUDE.md, if it exists in the working directory]
```

The task notes are the primary instruction. Everything else is orientation context. This is why well-written task notes matter — they're literally the prompt.

## High-level technical approach

### Crate: `crates/wrk/`

New binary crate in the saku workspace. Depends on:
- `saku-tdo` (as a library) — for reading/writing tasks, store access
- `std::process::Command` — for spawning `claude --print`
- `tokio` — for the daemon loop and concurrent execution

### Key components

1. **Task picker** — queries the tdo store for executable tasks. Filters by assignment, status, blockers. Returns tasks in priority order (respecting dependencies, deadlines, then creation order).

2. **Prompt builder** — assembles the claude prompt from task data. Injects tdo context, task notes, subtasks, and project-level instructions.

3. **Executor** — spawns `claude --print -p <prompt>` as a subprocess. Captures stdout/stderr. Handles timeouts. Returns success/failure + output.

4. **Reporter** — writes results back to tdo. Marks tasks done or adds error notes. Optionally triggers sync.

5. **Daemon loop** — ticker that calls pick → execute → report on interval. Manages concurrency limits. Handles graceful shutdown (finish current task before exit).

### Storage

wrk stores execution logs locally (not in tdo). Something like `~/.local/share/saku/wrk/logs/<task-number>-<timestamp>.log`. These are ephemeral — the important state (task status, completion notes) lives in tdo.

### Sync interaction

wrk reads from the local tdo store. If saku-sync is configured, tasks assigned from another device will appear after sync. wrk doesn't manage sync — it just reads whatever tdo has locally. The daemon could optionally trigger a sync poll before each check cycle.

## What wrk is NOT

- Not a chat interface. No conversational loop, no memory across tasks.
- Not a platform. No plugins, no integrations beyond tdo + claude.
- Not a scheduler. It doesn't decide *what* to work on — the human does, by assigning tasks. wrk just executes.
- Not a replacement for Claude Code. It shells out to claude. It's an orchestrator, not an AI runtime.

## Open questions

1. **Working directory** — should each task specify a repo/directory, or does wrk always run in the current project? Probably needs a `--dir` flag or a project-level config in tdo.
2. **Model selection** — always use the default claude model, or allow per-task `--model` override?
3. **Approval gates** — some tasks might need human review before the agent's changes are committed/pushed. The `--review` flag handles this for `wrk run`, but how does the daemon handle it?
4. **Failure retry** — should wrk retry failed tasks? Probably not automatically — leave them in_progress for the human to decide.
5. **Cost awareness** — long-running claude executions cost money. Should wrk have a per-task token/cost limit?
