# Saku — Roadmap

This roadmap flows directly from the [philosophy](PHILOSOPHY.md). Every priority is justified by whether it closes a loop — whether it makes the human-agent team more coherent, not just whether it adds features.

---

## Where We Are

**tdo v0.5.11** — The daily loop's work queue is solid. Task management, projects, areas, tags, deadlines, batch operations, task dependencies (data model), JSON/CSV output. The human-side is good. The agent-side is implicit.

**hbt** — Designed. Design spec exists in `documentation/hbt/`. The human rhythms loop is ready to build.

**Everything else** — The knowledge loop, work loop, communication loop, and exploration loop are entirely unbuilt. The suite's value multiplier doesn't exist yet.

**The gap** — tdo is a good personal task manager. It is not yet the coordination layer the philosophy describes. The loops are named but not connected.

---

## Milestone 0 — Prove the Daily Loop Thesis (~2 weeks, tdo only)

These changes make the human-agent handoff real without building anything new. They test whether the philosophy holds in practice before over-building.

### 0.1 Execution notes on completion

```bash
tdo done 42 --note "Refactored token refresh logic. 3 files changed. Tests pass."
```

When a task is completed by human or agent, an optional note records what was done. The logbook displays these. This is the audit trail that makes agent work reviewable. Without it, the logbook is just a list of completions — with it, it's a record of decisions.

### 0.2 `--ready` filter — dependency-free tasks

```bash
tdo view today --ready        # tasks the agent can start right now
tdo view inbox --ready        # agent-eligible items with no blockers
```

`depends_on` is already in the data model. This is one filter clause. But it enables parallel agent execution — the agent asks "what can I work on right now?" and gets a precise answer.

### 0.3 Agent conventions documented

No code changes. Name the conventions in SKILL.md:

- `--tag agent` → task is agent-executable
- `--tag needs-review` → agent created this, human should review
- Agent reads: `tdo view today --tag agent --ready`
- Human reviews: `tdo view inbox --tag needs-review`

Naming the protocol gives both principals a shared vocabulary.

### 0.4 Note preview in list views

```
  42  ○  Refactor auth token logic             Work / auth-service
         └ Consolidate refresh logic in token.rs:142 and :287 into one fn
```

A task with notes shows the first line dimmed beneath it. This signals "this task is well-specified and ready to hand off." A task without notes signals "this needs more spec before anyone can execute it."

---

## Milestone 1 — Complete the Daily Loop in tdo (~4-6 weeks)

Before adding new tools, the daily loop's work queue needs to be excellent on its own. These features complete tdo's core value.

### 1.1 Search

```bash
tdo search "auth"
tdo search "auth" --notes
```

A task list past 50 items is unusable without search. Table stakes.

### 1.2 Filter flags on view commands

```bash
tdo view today --project auth-service
tdo view all --tag urgent --area work
tdo view today --tag agent --ready       # the agent's primary command
```

Filters are also what allow the agent to have a scoped, predictable view of its own work.

### 1.3 Priority field

```bash
tdo add "Fix memory leak" --priority high
tdo move 42 --priority high
```

Color-coded in views. Sorts after deadline, before project grouping. Gives the ordering formula a human signal beyond deadline proximity.

### 1.4 Defer until

```bash
tdo add "Review Q2 plan" --defer-until 2026-03-01
tdo view deferred
```

The field exists in the model. Expose it. Tasks hidden until their date — useful for seasonal work and for agent tasks that become relevant when an external dependency resolves.

### 1.5 Dependency CLI flags

```bash
tdo add "Deploy" --depends-on 41
tdo add "Write tests" --blocks 55
```

The data model is there. Surface it in mutation commands. The `--ready` filter (M0) already uses it — now let humans and agents create these relationships explicitly.

### 1.6 Recurring tasks

```bash
tdo add "Weekly review" --every monday
tdo add "Pay rent" --every "1st of month"
```

The single highest-friction gap in tdo today. Manually recreating repeating tasks is the most common daily annoyance.

### 1.7 Enhanced date parser

```bash
tdo add "Q2 planning" --deadline eom
tdo add "Arch review" --when +2w
```

Small surface area, high daily impact.

---

## Milestone 2 — tdo as the Coordination Layer (~4-6 weeks)

With tdo solid as a human tool, these changes make the human-agent handoff first-class.

### 2.1 `tdo context` — the situational snapshot

```bash
tdo context
tdo context --json
```

One command that gives an agent its complete orientation: active projects, today's tasks, what's blocked, what's ready, recent completions, inbox items needing review. An agent starts every session with this.

```
  Context · Feb 22

  Active projects: auth-service, dashboard-v2, docs
  Today: 5 tasks (2 agent, 3 human)
  Ready now: 3 (no unmet dependencies)
  Blocked: 2 (waiting on #41, #38)
  Inbox needs review: 4 items
  Completed last 48h: 6 tasks
```

### 2.2 Assignment field

```bash
tdo add "Refactor auth module" --assign agent
tdo add "Call the client" --assign human
tdo view today --mine
tdo view today --agent-queue
```

Logbook attribution: records who completed each task. Enables "show me everything the agent did today."

### 2.3 Fuzzy match confirmation

```bash
tdo done "review"
  Found: "Review PR for auth refactor" (#42)
  Complete this task? [y/N]:
```

Agents use IDs; humans use fuzzy names. Dangerous for destructive ops without confirmation. `--force` skips for scripting.

### 2.4 Checklist CLI access

```bash
tdo add "Deploy to production" --checklist "run tests,check staging,update docs,notify team"
tdo check 42 1
tdo view task 42
```

Data model exists. Expose it. Agents use checklists to record granular progress on long tasks — especially important for resumable, interrupted work.

---

## Milestone 3 — The Knowledge Loop: `nte` + `dcs`

The knowledge loop is the highest-value loop after the daily loop. An agent that can read what the team knows and why decisions were made doesn't repeat past mistakes and doesn't ask questions already answered.

### 3.1 `nte` — Notes

Evergreen reference. Architecture docs, runbooks, how-things-work. Linked from tdo tasks via a note reference. Structured around the same areas and projects as tdo.

```bash
nte add "Auth architecture" --project auth-service
nte add "How to deploy to staging" --project ops --tag runbook
nte view project "auth-service"
nte search "token refresh"
```

Agent use: read before starting work on a project. Write when discovering something that should be remembered. Human use: reference while working, review after agent sessions to see what it learned.

Key constraint: `nte` notes are evergreen — they get updated, not replaced. The journal (`jrn`) is where ephemeral dated entries go. Notes are the permanent layer.

### 3.2 `dcs` — Decision Log

Structured records of choices: what was decided, why, what alternatives were considered, what the consequence is expected to be. The architecture decision record (ADR) format, but terminal-native and queryable.

```bash
dcs add "Use JWT for auth" \
    --context "Considered cookie sessions; JWT fits stateless edge deployment" \
    --alternatives "cookie sessions, opaque tokens" \
    --consequence "Need token refresh strategy; can't invalidate instantly"
dcs search "auth"
dcs view project "auth-service"
```

Agent use: before proposing a change to a system, reads related decisions. Doesn't re-litigate closed questions. Human use: answers "why is it like this?" instantly.

Key constraint: decisions are immutable records, not editable notes. A changed decision is a new decision that supersedes the old one.

---

## Milestone 4 — The Work Loop: `ctx` + `tmr`

### 4.1 `ctx` — Session Context

The handoff tool. Saves where you were — in your thinking, not just on the task list — so that whoever picks it up next (you after a break, or an agent at session start) doesn't lose the thread.

```bash
ctx save "Working on auth refresh edge case. Next: reproduce bug in test_token.rs:142. Blocked: need to understand jiff timezone offset behavior."
ctx save --for agent "Pick up from here. Task #42 is ready. Reproduce test failure first, then fix."
ctx show
ctx history
```

Human use: come back after a meeting and immediately remember where you were. Agent use: `ctx show` at session start; `ctx save` with completion notes after. This is the async handoff protocol for sessions.

Key distinction from `tdo`: context is *mental state*, not task state. "Where was I in my thinking" vs. "what's on the list."

### 4.2 `tmr` — Time Tracker

Pomodoro-style focus sessions with time-on-task tracking. Links to tdo task IDs.

```bash
tmr start 42           # start timer for task #42
tmr stop               # stop, record time
tmr view today         # time breakdown today
tmr view week          # weekly summary by project
```

Human use: focus sessions, capacity planning, understanding where time actually goes. Agent use: records time spent on tasks it executes — useful for estimating future work.

---

## Milestone 5 — The Communication Loop: `msg` + `ppl`

### 5.1 `msg` — Async Waiting

Tracks things blocked on external parties. The gap every other system misses: work you're waiting on from people outside the team has no home. It drops.

```bash
msg add "Design review for dashboard" --from alice --sent 2026-02-20 --by 2026-02-25
msg add "Security audit sign-off" --from security-team
msg view             # everything I'm waiting on, sorted by how overdue
msg done 3           # mark as received
```

Agent use: when the agent hits an external dependency, it creates a `msg` entry and surfaces it in the human's review queue. Human processes it, unblocks the agent's work.

### 5.2 `ppl` — People Context

Lightweight context about the people you work with. Not a CRM — a working memory layer.

```bash
ppl add "Alice Chen" --role "Design Lead" --note "Reviews code on Thursdays. Prefers async."
ppl note alice "Agreed to send design specs by March 1. This unblocks frontend PR."
ppl view alice
```

Human use: before a 1:1, instant context. After a meeting, capture what was agreed. Agent use: reads `ppl` to understand communication preferences and pending commitments.

---

## Milestone 6 — Human Rhythms: `hbt`

Already designed. The human rhythms loop. Daily tracking with GitHub-style heatmap. Agent-independent — this loop belongs to the human.

```bash
hbt log exercise
hbt view year
hbt view stats
```

---

## The Orchestrator: `saku`

Built last, after tools exist to orchestrate. The cross-tool layer that makes the suite more than the sum of its parts.

```bash
saku context           # full situational picture across all tools
saku context --json    # structured for agent consumption
saku search "auth"     # search across tdo, nte, jrn, dcs simultaneously
saku today             # today from tdo + jrn + cal combined
saku sync              # sync everything
saku status            # counts and health across all tools
```

`saku context --json` is the single most important agent command in the whole suite. It gives a new session everything it needs to orient without reading each tool individually.

---

## What We're Explicitly Not Prioritizing

**TUI/interactive mode** — Agents don't use it. Build it only after every loop has a solid CLI.

**Mobile/web companion** — Out of scope for v1. The CLI is the interface.

**Import from other tools** — Useful for user acquisition, not for the philosophy. A user who cares about the human-agent workflow will set up Saku fresh.

**Color customization / theming** — Build the right defaults. Resist configurability.

**Plugin/webhook system** — The CLI and filesystem are the plugin system. Don't add another layer.

**`bkm` (bookmarks)** — Reconsidered. A bookmark is a note with a URL. Absorb into `nte` with a `--url` flag and a `--tag ref`. Don't build a separate tool for this.

---

## Priority Summary

| Milestone | Loop | What | Rationale |
|---|---|---|---|
| **M0** | Daily | Execution notes, `--ready`, agent conventions, note previews | Prove the thesis with minimal code |
| **M1** | Daily | Search, filters, priority, defer, dependencies, recurrence, dates | Complete tdo as a human tool |
| **M2** | Daily | `tdo context`, assignment, fuzzy confirmation, checklists | Make coordination explicit and first-class |
| **M3** | Knowledge | `nte`, `dcs` | Give the agent memory and decision history |
| **M4** | Work | `ctx`, `tmr` | Make session handoff and focus first-class |
| **M5** | Communication | `msg`, `ppl` | Stop dropping things blocked on external parties |
| **M6** | Human Rhythms | `hbt` | Complete the suite |
| **`saku`** | Orchestrator | Cross-tool context, search, sync | Build when tools exist to orchestrate |

---

## How to Read This Roadmap

Each milestone is a complete, shippable increment. M0 can ship in a week and makes the human-agent workflow real. M1 makes tdo competitive with any CLI task manager. M2 makes it uniquely Saku. M3 is when the suite starts earning its name.

Resist the temptation to start M3 before M1 is done. The suite multiplies tdo's value — it doesn't replace it. A weak foundation means every tool on top is weaker too.
