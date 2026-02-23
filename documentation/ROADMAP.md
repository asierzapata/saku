# Saku — Roadmap

This roadmap flows directly from the [philosophy](PHILOSOPHY.md). Every priority is justified by whether it makes Saku better as a coordination layer for human-agent teams, not just as a personal todo app.

---

## Where We Are

**tdo v0.5.11** — The core tool is solid. Task management, projects, areas, tags, deadlines, batch operations, task dependencies (data model), JSON/CSV output. Good fundamentals.

**The rest of the suite** — Unbuilt. `hbt` has a design spec. `nte`, `cal`, `tmr`, `bkm` are planned stubs.

**The gap** — tdo is well-designed for a single human user. The human-agent coordination layer is mostly implicit. The conventions exist but aren't first-class.

---

## Milestone 0 — Prove the Thesis (tdo, ~2 weeks)

These changes make the human-agent workflow real without introducing new abstractions. They test whether the philosophy holds in practice.

### 0.1 Execution notes on completion

```bash
tdo done 42 --note "Refactored token refresh logic. 3 files changed. Tests pass."
```

When a task is completed (by human or agent), an optional note records what was done. The logbook displays these notes. This is the audit trail that makes agent work reviewable.

**Why first:** Zero new concepts. One new flag. Immediately unlocks the "human reviews what agent did" workflow.

### 0.2 `--ready` filter: dependency-free tasks

```bash
tdo view today --ready        # only tasks with no unmet dependencies
tdo view inbox --ready        # tasks the agent can start on immediately
```

For an agent executing work in parallel, this is the primary command. It tells the agent "here is what you can pick up right now without waiting for anything."

**Why now:** `depends_on` is already in the data model. This is one filter clause in the query layer.

### 0.3 Document the agent conventions in SKILL.md

No code changes. Establish and document two conventions:

- `--tag agent` marks a task as agent-executable
- `--tag needs-review` marks a task the agent created for human attention
- Agents read: `tdo view today --tag agent --ready`
- Humans review: `tdo view inbox --tag needs-review`

These are just tags — they have zero special treatment in the code today. But naming them gives agents and humans a shared vocabulary.

### 0.4 Note preview in list views

```
  42  ○  Refactor auth token logic             Work / auth-service
         └ Consolidate refresh logic in token.rs:142 and :287 into single fn
```

If a task has notes, show the first line dimmed below the task line (collapsed by default, opt-in via `--notes`). This signals "this task is well-specified" at a glance.

**Why it matters:** A human scanning their list can immediately see which tasks are ready to hand off to an agent (they have notes) and which need more spec.

---

## Milestone 1 — tdo as a Great Human Tool (~4-6 weeks)

Before the human-agent workflow can work well, the tool needs to be excellent for the human alone. These are the features that complete tdo's core value proposition.

### 1.1 Search

```bash
tdo search "auth"              # search titles and notes
tdo search "auth" --notes      # include note content
```

A task list with 50+ tasks is unusable without search. This is table stakes.

### 1.2 Filter flags on view commands

```bash
tdo view today --project auth-service
tdo view all --tag urgent --area work
tdo view today --tag agent --ready    # (combines with Milestone 0)
```

Filters make large task lists navigable. Also the foundation for the agent's scoped view of its own work.

### 1.3 Priority field

```bash
tdo add "Fix memory leak" --priority high
tdo move 42 --priority high
```

Display: color-coded in views (red high, yellow medium, default low). Sort: after deadline, before project grouping.

**Why not before search/filter:** Priority without the ability to filter by it is less useful. Search and filter make priority actionable.

### 1.4 Defer until (expose existing model field)

```bash
tdo add "Review Q2 plan" --defer-until 2026-03-01
tdo view deferred              # tasks hidden until their defer date
```

The `defer_until` field already exists in the data model. Expose it via CLI. Hidden tasks don't appear in views until their date arrives. Useful for both humans (seasonal tasks) and agents (tasks that become relevant when a dependency outside the system resolves).

### 1.5 Dependency CLI flags

```bash
tdo add "Deploy" --depends-on 41         # can't do this until #41 is done
tdo add "Write tests" --blocks 55        # #55 is blocked until this is done
tdo view project "auth-service" --show-blocked
```

The data model is there. The `--ready` filter (Milestone 0) already uses it. Now expose it in the mutation commands.

---

## Milestone 2 — tdo as the Coordination Layer (~6-8 weeks)

With the human workflow solid and the agent conventions established, these features make the human-agent handoff explicit and robust.

### 2.1 `tdo context` — the situational snapshot

```bash
tdo context                    # full picture as human-readable text
tdo context --json             # structured snapshot for agent consumption
```

Outputs a single summary of the current state: active projects, today's tasks, what's blocked, what's ready, recent completions, what's in inbox for review. An agent runs this at the start of a session to orient itself without reading every view.

Example output:
```
  Context · Feb 22

  Active projects: auth-service, dashboard-v2, docs
  Today: 5 tasks (2 agent-tagged, 3 human)
  Ready to execute: 3 (no unmet dependencies)
  Blocked: 2 (waiting on: #41, #38)
  Inbox: 4 tasks (2 need-review from agent)
  Recent: Completed 6 tasks in last 48h
```

### 2.2 Assignment field

Move from convention (tags) to first-class field:

```bash
tdo add "Refactor auth module" --assign agent
tdo add "Call the client" --assign human
tdo view today --mine          # human's tasks
tdo view today --agent-queue   # agent's tasks
```

Logbook attribution: completed tasks record whether a human or agent completed them. Enables "show me everything the agent did today."

### 2.3 Fuzzy match confirmation

```bash
tdo done "review"
  Found: "Review PR for auth refactor" (#42)
  Complete this task? [y/N]:
```

Currently fuzzy matches execute silently — dangerous for humans, fine for agents (they use IDs). Add confirmation for human-triggered fuzzy matches; agents pass `--force` to skip.

### 2.4 Checklist CLI access

```bash
tdo add "Deploy to production" --checklist "run tests,check staging,update docs,notify team"
tdo check 42 1                 # check item 1 of task 42
tdo view task 42               # show task with checklist progress
```

Checklists already exist in the data model. Expose them. For agents executing multi-step tasks, checklists let them record granular progress — useful when a long task is interrupted and resumed.

---

## Milestone 3 — Recurring and Temporal (parallel track, ~4 weeks)

These can be built in parallel with Milestone 2. They're self-contained.

### 3.1 Recurring tasks

```bash
tdo add "Weekly review" --every monday
tdo add "Pay rent" --every "1st of month"
tdo add "Check logs" --every day --project ops
```

When a recurring task is completed, the next instance is automatically created. This is the single most-requested feature and the one that causes the most daily friction (manually recreating tasks).

### 3.2 Enhanced date parser

```bash
tdo add "Q2 planning" --deadline eom       # end of month
tdo add "Arch review" --when +2w           # 2 weeks from now
tdo add "Tax filing" --deadline eoq        # end of quarter
```

Small quality-of-life improvement with high daily impact.

---

## Milestone 4 — The Suite Begins (~quarter)

Once tdo is solid, the suite's value multiplies when the other tools exist.

### 4.1 `nte` — Note taking

The agent's long-term memory. Architecture decisions, runbooks, research notes. Linked from tdo tasks via `--note-ref`. Agents write to `nte` when they discover something that should be remembered; humans read it for context.

Key design constraint: notes are linked to projects/areas, mirroring tdo's structure. When you view a project in tdo, you can surface related notes from `nte`.

### 4.2 Sync exposed to users

`saku-sync` exists but is not user-facing. Expose it with sensible defaults. The human's laptop and their cloud dev environment should share the same task state.

### 4.3 Cross-tool `saku context`

A suite-level command that gives a complete situational picture:

```bash
saku context --json
```

Returns: today's tdo tasks, relevant nte notes for active projects, upcoming cal events affecting deadlines. One command, one JSON blob — the agent's full orientation at session start.

---

## What We're Explicitly Not Prioritizing

**TUI/interactive mode** — Keyboard-driven terminal UI is nice but not the bottleneck. Agents don't use it. Add it only after the coordination layer is solid and human UX is smooth.

**Mobile/web companion** — Out of scope for v1. The CLI is the interface.

**Import from other tools** — Useful for user acquisition but not for the philosophy. A user who cares about the human-agent workflow will set up tdo fresh; they won't have a Todoist import to migrate.

**Color customization / theming** — Build the right defaults. Resist the urge to make everything configurable.

**Plugin/webhook system** — Composability happens at the CLI and filesystem level. That's the plugin system. Don't add another layer.

---

## Priority Summary

| Milestone | What | When | Why |
|---|---|---|---|
| **0** | Execution notes, `--ready`, agent conventions, note previews | Now | Prove the thesis with minimal code |
| **1** | Search, filter flags, priority, defer-until, dependency flags | Next 4-6w | Make tdo excellent for the human |
| **2** | `tdo context`, assignment field, fuzzy confirmation, checklist CLI | Following 6-8w | Make coordination explicit |
| **3** | Recurring tasks, enhanced dates | Parallel to M1/M2 | Fix the top daily friction point |
| **4** | `nte`, sync, `saku context` | Quarter | Build the suite |

---

## How to Read This Roadmap

Each milestone is a complete, shippable increment. Milestone 0 can be shipped in a week and immediately makes the human-agent workflow real. Milestone 1 makes tdo competitive with any CLI task manager. Milestone 2 makes it uniquely Saku. Milestone 4 makes it a suite.

Resist the temptation to start Milestone 4 before Milestone 1 is done. The suite multiplies tdo's value — it doesn't replace it.
