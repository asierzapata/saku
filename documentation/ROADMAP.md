# Saku — Roadmap

This roadmap flows directly from the [philosophy](PHILOSOPHY.md). Every priority is justified by whether it closes a loop — whether it makes the human-agent team more coherent, not just whether it adds features.

---

## Where We Are

**tdo v0.9.0** — The daily loop's work queue is solid. Task management, projects, areas, tags, deadlines, batch operations, task dependencies (CLI + data model), recurring tasks, sync support. JSON/CSV output. The human-side is good. The agent-side is implicit.

**Milestone 1 Progress:**
- ✅ Priority field implemented (high/medium/low)
- ✅ Dependency CLI flags (`tdo depend`, `--parent` for subtasks)
- ✅ Recurring tasks (`--every`, `--until`)
- ✅ Enhanced date parser (natural language + ISO dates)
- ✅ JSON/CSV output modes
- ⏳ Search (not yet implemented)
- ⏳ Filter flags on view commands (not yet implemented)
- ⏳ Defer until (data model exists, CLI not fully exposed)

**hbt** — Designed. Design spec exists in `documentation/hbt/`. The human rhythms loop is ready to build.

**Everything else** — The knowledge loop, work loop, communication loop, and exploration loop are entirely unbuilt. The suite's value multiplier doesn't exist yet.

**The gap** — tdo is a good personal task manager. It is not yet the coordination layer the philosophy describes. The loops are named but not connected.

---

## Milestone 0 — Prove the Daily Loop Thesis (~2 weeks, tdo only)

These changes make the human-agent handoff real without building anything new. They test whether the philosophy holds in practice before over-building.

**Status: Partially complete. Items 0.2, 0.3, and 0.4 still needed.**

### 0.1 Execution notes on completion ⏳

```bash
tdo done 42 --note "Refactored token refresh logic. 3 files changed. Tests pass."
```

When a task is completed by human or agent, an optional note records what was done. The logbook displays these. This is the audit trail that makes agent work reviewable. Without it, the logbook is just a list of completions — with it, it's a record of decisions.

**Current status:** Task notes exist, but `--note` flag on `done` command not yet implemented.

### 0.2 `--ready` filter — dependency-free tasks ⏳

```bash
tdo view today --ready        # tasks the agent can start right now
tdo view inbox --ready        # agent-eligible items with no blockers
```

`depends_on` is already in the data model. This is one filter clause. But it enables parallel agent execution — the agent asks "what can I work on right now?" and gets a precise answer.

**Current status:** Dependencies implemented, but `--ready` filter not yet added to view commands.

### 0.3 Agent conventions documented ⏳

No code changes. Name the conventions in SKILL.md:

- `--tag agent` → task is agent-executable
- `--tag needs-review` → agent created this, human should review
- Agent reads: `tdo view today --tag agent --ready`
- Human reviews: `tdo view inbox --tag needs-review`

Naming the protocol gives both principals a shared vocabulary.

**Current status:** Tags work, but agent conventions not formally documented in SKILL.md yet.

### 0.4 Note preview in list views ⏳

```
  42  ○  Refactor auth token logic             Work / auth-service
         └ Consolidate refresh logic in token.rs:142 and :287 into one fn
```

A task with notes shows the first line dimmed beneath it. This signals "this task is well-specified and ready to hand off." A task without notes signals "this needs more spec before anyone can execute it."

**Current status:** Not yet implemented.

---

## Milestone 1 — Complete the Daily Loop in tdo (~4-6 weeks)

Before adding new tools, the daily loop's work queue needs to be excellent on its own. These features complete tdo's core value.

**Status: Mostly complete. Missing search, filter flags, and defer-until CLI.**

### 1.1 Search ⏳

```bash
tdo search "auth"
tdo search "auth" --notes
```

A task list past 50 items is unusable without search. Table stakes.

**Current status:** Not yet implemented.

### 1.2 Filter flags on view commands ⏳

```bash
tdo view today --project auth-service
tdo view all --tag urgent --area work
tdo view today --tag agent --ready       # the agent's primary command
```

Filters are also what allow the agent to have a scoped, predictable view of its own work.

**Current status:** Not yet implemented.

### 1.3 Priority field ✅

```bash
tdo add "Fix memory leak" --priority high
tdo move 42 --priority high
```

Color-coded in views. Sorts after deadline, before project grouping. Gives the ordering formula a human signal beyond deadline proximity.

**Current status:** ✅ Implemented.

### 1.4 Defer until ⏳

```bash
tdo add "Review Q2 plan" --defer-until 2026-03-01
tdo view deferred
```

The field exists in the model. Expose it. Tasks hidden until their date — useful for seasonal work and for agent tasks that become relevant when an external dependency resolves.

**Current status:** Field exists in data model but not fully exposed in CLI.

### 1.5 Dependency CLI flags ✅

```bash
tdo add "Deploy" --depends-on 41
tdo add "Write tests" --blocks 55
```

The data model is there. Surface it in mutation commands. The `--ready` filter (M0) already uses it — now let humans and agents create these relationships explicitly.

**Current status:** ✅ Implemented via `tdo depend` command and `--parent` flag.

### 1.6 Recurring tasks ✅

```bash
tdo add "Weekly review" --every monday
tdo add "Pay rent" --every "1st of month"
```

The single highest-friction gap in tdo today. Manually recreating repeating tasks is the most common daily annoyance.

**Current status:** ✅ Implemented via `--every` and `--until` flags.

### 1.7 Enhanced date parser ✅

```bash
tdo add "Q2 planning" --deadline eom
tdo add "Arch review" --when +2w
```

Small surface area, high daily impact.

**Current status:** ✅ Implemented. Supports natural language (today, tomorrow, monday, next-week) and ISO dates.

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

## Milestone 3 — The Knowledge Loop: `dcs`

The knowledge loop is the highest-value loop after the daily loop. An agent that can read why decisions were made doesn't repeat past mistakes and doesn't re-propose what was already rejected.

### 3.1 `dcs` — Decision Log

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

## Milestone 4 — The Work Loop: `ctx`

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

---

## The Orchestrator: `saku`

Built last, after tools exist to orchestrate. The cross-tool layer that makes the suite more than the sum of its parts.

```bash
saku context           # full situational picture across all tools
saku context --json    # structured for agent consumption
saku search "auth"     # search across tdo, jrn, dcs simultaneously
saku today             # today from tdo + jrn combined
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
| **M3** | Knowledge | `dcs` | Give the agent decision history so it doesn't re-litigate closed questions |
| **M4** | Work | `ctx` | Make session handoff first-class |
| **`saku`** | Orchestrator | Cross-tool context, search, sync | Build when tools exist to orchestrate |

---

## How to Read This Roadmap

Each milestone is a complete, shippable increment. M0 can ship in a week and makes the human-agent workflow real. M1 makes tdo competitive with any CLI task manager. M2 makes it uniquely Saku. M3 is when the suite starts earning its name.

Resist the temptation to start M3 before M1 is done. The suite multiplies tdo's value — it doesn't replace it. A weak foundation means every tool on top is weaker too.
