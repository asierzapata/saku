# wrk — Agent Task Executor Setup

`wrk` is the agent task executor for the saku productivity suite. It picks up tasks from `tdo` that are assigned to agents and executes them via the `claude` CLI.

## Overview

**Components:**

- **tdo** — Task queue where tasks are created and assigned to agents
- **wrk** — Daemon (or one-shot runner) that picks up agent-assigned tasks, executes them via `claude`, and reports results back to `tdo`
- **claude** — The Claude Code CLI (`claude --print`), used as the execution backend

**Workflow:**

```
Human creates task in tdo → assigns to "agent" → wrk picks it up →
wrk builds prompt → wrk calls `claude --print` → wrk reports result back to tdo
```

## How It Works

### Architecture

wrk is organized as a pipeline:

```
picker → prompt → executor → reporter → logs
```

1. **Picker** — Scans the tdo store for tasks assigned to `"agent"` or `"wrk"` that are not completed, not deleted, and not blocked by dependencies. Sorts by deadline (soonest first), then task number.

2. **Prompt** — Assembles a multi-section prompt from the task data:
   - Context header (date, project, area, deadline)
   - Task title
   - Task notes (the primary instruction body)
   - Subtask checklist (with `[x]`/`[ ]` markers)
   - Blocker status (incomplete blocking tasks)
   - Project `CLAUDE.md` (if present in the working directory)

3. **Executor** — Spawns `claude --print -p <prompt>` as a subprocess in the specified working directory. Captures stdout/stderr. Default timeout: 30 minutes.

4. **Reporter** — Updates the task in the tdo store:
   - **Success** → marks the task complete, appends output summary to notes
   - **Needs review** → adds `needs-review` tag, appends summary, does not complete
   - **Failure** → appends error details to notes, leaves task in current state

5. **Logs** — Writes execution logs to `~/.local/share/saku/wrk/logs/` with format `{task_number}-{YYYYMMDD-HHMMSS}.log`.

### Data Flow

wrk reads and writes the same store file as tdo (`~/.local/share/tdo/store.json`). There is no separate database. When wrk updates a task, tdo's sync (if configured) automatically picks up the change.

```
~/.local/share/tdo/store.json   ← shared between tdo and wrk
~/.local/share/saku/wrk/logs/   ← wrk execution logs
```

### Task Lifecycle

```
tdo add "Implement feature X" -p myproject
tdo assign 42 agent                          # marks task for agent execution

# wrk picks it up:
# 1. Claims: sets assigned_to="wrk", moves to Today if in Inbox/Someday
# 2. Executes: runs claude --print with assembled prompt
# 3. Reports: marks complete (or needs-review, or failure)

tdo view 42                                  # see the result in notes
wrk log 42                                   # see full execution log
```

## CLI Reference

### `wrk run <task-number>`

Execute a single task immediately.

```bash
wrk run 42                     # execute task #42
wrk run 42 --dry               # print the prompt without executing
wrk run 42 --review            # mark as "needs review" instead of "done"
wrk run 42 --dir /path/to/repo # set the working directory
```

| Flag | Default | Description |
|---|---|---|
| `--dry` | off | Print the assembled prompt and exit |
| `--review` | off | On success, tag with `needs-review` instead of completing |
| `--dir <PATH>` | current directory | Working directory for claude execution |

### `wrk daemon`

Long-running process that polls for agent-assigned tasks.

```bash
wrk daemon                              # poll every 60s, 1 task at a time
wrk daemon --poll 30s --max-concurrent 3 # poll every 30s, up to 3 parallel
wrk daemon --once                       # single poll cycle, then exit
wrk daemon --dir /path/to/repo          # set the working directory
```

| Flag | Default | Description |
|---|---|---|
| `--poll <DURATION>` | `60s` | Poll interval (`30s`, `5m`, etc.) |
| `--once` | off | Run a single poll cycle and exit |
| `--max-concurrent <N>` | `1` | Maximum parallel task executions |
| `--dir <PATH>` | current directory | Working directory for claude execution |

The daemon:
- Reloads the tdo store fresh each cycle
- Claims tasks before spawning execution (prevents duplicate work)
- Uses a semaphore for concurrency control
- Reads `CLAUDE.md` from the working directory once at startup

### `wrk status`

Show recent execution history.

```bash
wrk status                # last 5 entries
wrk status --history      # last 20 entries
```

### `wrk log <task-number>`

View the full execution log for the most recent run of a task.

```bash
wrk log 42
```

## Prerequisites

- **claude** CLI must be installed and on `PATH`. wrk invokes it as `claude --print -p <prompt>`.
- **tdo** store must exist (`~/.local/share/tdo/store.json`). Run any `tdo` command first to initialize it.

## Installation

### From source (requires Rust 1.88+)

```bash
cargo install --path crates/wrk
```

Or build with the workspace:

```bash
cargo build --release -p saku-wrk
# binary: target/release/wrk
```

### Running locally

```bash
# One-shot: execute a specific task
wrk run 42 --dir /path/to/your/project

# Daemon: poll continuously
wrk daemon --dir /path/to/your/project --poll 30s
```

## Self-Hosted Deployment (Docker)

For running wrk as a persistent daemon on a server or dedicated machine.

### Docker Image

Build the image from the repository root:

```bash
docker build -f crates/wrk/Dockerfile -t wrk .
```

The Dockerfile uses a two-stage build:

1. **Builder** — Compiles `wrk` in `rust:1.88-slim`
2. **Runtime** — Copies the binary into `debian:bookworm-slim` with only `ca-certificates`

### Docker Run

```bash
docker run -d \
  --name wrk \
  -v /path/to/tdo/store:/data/tdo \
  -v /path/to/your/project:/workspace \
  -e TDO_DATA_DIR=/data/tdo \
  wrk daemon \
    --dir /workspace \
    --poll 60s \
    --max-concurrent 2
```

**Volume mounts:**

| Mount | Container path | Purpose |
|---|---|---|
| tdo data directory | `/data/tdo` | Shared task store (`store.json`) |
| Project directory | `/workspace` | Working directory for claude (reads `CLAUDE.md`, runs in this context) |
| wrk logs (optional) | `/data/wrk/logs` | Persist execution logs across container restarts |

### Docker Compose

For a typical setup running wrk alongside the saku sync server:

```yaml
# crates/wrk/docker-compose.yml
services:
  wrk:
    build:
      context: ../..
      dockerfile: crates/wrk/Dockerfile
    volumes:
      - tdo-data:/data/tdo
      - wrk-logs:/data/wrk/logs
      - /path/to/your/project:/workspace:ro
    environment:
      - TDO_DATA_DIR=/data/tdo
      - WRK_LOG_DIR=/data/wrk/logs
    command: ["daemon", "--dir", "/workspace", "--poll", "60s", "--max-concurrent", "2"]
    restart: unless-stopped

volumes:
  tdo-data:
  wrk-logs:
```

Start it:

```bash
cd crates/wrk
docker compose up -d
```

View logs:

```bash
docker compose logs -f wrk
```

### Combining with saku-server

If you already run saku-server with Docker Compose, you can add wrk to the same compose file. The key is sharing the tdo data volume:

```yaml
services:
  saku-server:
    # ... existing saku-server config ...
    volumes:
      - server-data:/data
      - ./config.toml:/etc/saku-server/config.toml:ro

  wrk:
    build:
      context: ../..
      dockerfile: crates/wrk/Dockerfile
    volumes:
      - tdo-data:/data/tdo
      - /path/to/project:/workspace:ro
    environment:
      - TDO_DATA_DIR=/data/tdo
    command: ["daemon", "--dir", "/workspace", "--poll", "60s"]
    restart: unless-stopped

volumes:
  server-data:
  tdo-data:
```

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `TDO_DATA_DIR` | `~/.local/share/tdo` | Path to the tdo data directory (contains `store.json`) |

### Important: claude CLI in Docker

The Docker container needs the `claude` CLI available. You have two options:

**Option A: Mount claude from the host**

```bash
docker run -d \
  -v $(which claude):/usr/local/bin/claude:ro \
  -v $HOME/.claude:/root/.claude:ro \
  # ... other flags ...
  wrk daemon --dir /workspace
```

**Option B: Install claude in the image**

Add to the Dockerfile runtime stage:

```dockerfile
RUN npm install -g @anthropic-ai/claude-code
```

This requires Node.js in the runtime image. See the Dockerfile comments for details.

**Option C: Use the API directly (future)**

A future version of wrk may support calling the Anthropic API directly, removing the claude CLI dependency.

## Systemd Service (Linux)

For running wrk directly on a Linux host without Docker:

```ini
# /etc/systemd/system/wrk.service
[Unit]
Description=wrk agent task executor
After=network.target

[Service]
Type=simple
User=youruser
WorkingDirectory=/path/to/your/project
ExecStart=/usr/local/bin/wrk daemon --poll 60s --max-concurrent 2
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now wrk
sudo journalctl -u wrk -f    # view logs
```

## Monitoring

### Execution logs

```bash
wrk status              # quick overview of recent executions
wrk status --history    # extended history (20 entries)
wrk log 42              # full output for task #42
```

Log files are stored at `~/.local/share/saku/wrk/logs/` (or `/data/wrk/logs` in Docker). Each file contains:

```
=== wrk execution log ===
Task:     #42
Duration: 123.5s
Exit:     0
===

--- stdout ---
[claude output]

--- stderr ---
[errors if any]
```

### Task state in tdo

After execution, check the task in tdo:

```bash
tdo view 42
```

- **Successful execution** → task is marked complete, notes contain `[wrk] Completed successfully.`
- **Needs review** → task has `needs-review` tag, notes contain `[wrk] Execution completed — needs review.`
- **Failed execution** → task remains open, notes contain `[wrk] Execution failed.` with error details

## Troubleshooting

**"Error: Failed to spawn claude process"**
The `claude` CLI is not installed or not on `PATH`. Install it and verify with `claude --version`.

**"No executable tasks found"**
No tasks are assigned to agents. Use `tdo assign <task-number> agent` to assign a task.

**"Error: Task #N is already completed"**
The task was already completed (possibly by a human or another wrk instance). No action needed.

**"Error: Task #N is deleted"**
The task was soft-deleted. Restore it with `tdo restore <task-number>` if you want to re-execute.

**Tasks are not being picked up by the daemon**
Check that:
1. The task's `assigned_to` field is `"agent"` or `"wrk"` (case-insensitive)
2. The task is not completed or deleted
3. The task is not blocked by incomplete dependencies (`tdo view <task-number>` shows blockers)

**Execution timeout (30 minutes)**
The default timeout is 30 minutes per task. Long-running tasks will be killed. Break large tasks into smaller subtasks.
