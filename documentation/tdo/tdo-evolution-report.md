# TDO Evolution Report

**Date:** February 22, 2026
**Version at time of writing:** v0.5.11
**Author:** PM Review

---

## 1. What We Have Today

### Core Data Model

A task in tdo carries:

| Field | Type | Notes |
|---|---|---|
| `title` | String | Required |
| `when` | Enum | Inbox / Scheduled / Someday |
| `deadline` | Optional date | Hard due date |
| `defer_until` | Optional date | Exists in model, not exposed to CLI |
| `project_id` | Optional UUID | |
| `area_id` | Optional UUID | |
| `tags` | Vec<String> | |
| `notes` | Optional String | |
| `checklist` | Vec<ChecklistItem> | Exists but editor-only |
| `depends_on` | Vec<UUID> | Data model done, CLI flags pending |
| `completed_at` | Optional Timestamp | |

### Task Ordering Algorithm (the "tdo formula")

Tasks in every view are sorted by the `order_tasks_impl` function in `crates/tdo/src/models/task.rs`. The current sort key is a strict priority ladder:

```
0. Blocked? → pushed to bottom (tasks with unmet depends_on)
1. Deadline urgency → sooner deadlines first, no deadline last
2. Project grouping → project tasks before inbox tasks
3. Area grouping → area tasks before unaffiliated tasks
4. Task number → oldest tasks first within a group
```

This is a **pure structural sort**. It knows nothing about how long a task takes, how important it is, or how much cognitive energy it requires.

### Shipped Features (v0.4.0 → v0.5.11)

| Feature | Version | Notes |
|---|---|---|
| Command structure (`tdo view today`) | v0.5 | All views under `tdo view <subcommand>` |
| Task dependencies data model | v0.5.9 | `depends_on` field + blocked sorting |
| Batch/bulk task mutations | v0.5.9 | Multiple IDs in done/delete/move |
| Deadlines view | v0.5.11 | `tdo view deadlines` grouped by urgency |
| JSON + CSV output | v0.5.11 | `--output json/csv` on view commands |
| Evening/Anytime cleanup | v0.5.x | Removed from code and docs |
| Clippy/dead code cleanup | v0.5.x | Clean compiler output |

---

## 2. Current Gaps (Open Backlog)

### P1 — High Impact, Not Yet Started

| Gap | Why it Matters |
|---|---|
| Search (`tdo search <query>`) | Unusable at 100+ tasks without grep piping |
| Filter flags on views (`--project`, `--tag`) | No way to focus a view on a subset |
| Fuzzy match confirmation prompt | Destructive operations (done, delete) fire silently |
| Priority field (`--priority high/med/low`) | No distinction between urgent and nice-to-have |
| Defer until CLI exposure | Field exists in model, but only settable via `tdo edit` |

### P2 — Power User Features

| Gap | Why it Matters |
|---|---|
| Recurring tasks (`--every monday`) | Manual re-creation is the biggest daily friction |
| CLI flags for dependencies (`--depends-on`, `--blocks`) | Data model is there; UX is missing |
| Checklist CLI commands | Can't mark sub-items done without opening an editor |
| Enhanced date parser (`+3d`, `eom`, `q2`) | Power users expect this shorthand |
| Import (Todoist, Things, CSV) | Blocks migration from other tools |

### P3 — Long Horizon

| Gap | Why it Matters |
|---|---|
| Interactive TUI mode | Efficiency for task-heavy workflows |
| Time estimates and tracking | Capacity planning, billing use cases |
| Sync (saku-sync exists but not user-facing) | Multi-device use |
| Config file (`~/.config/tdo/config.toml`) | Color theming, defaults |
| Subtasks / parent-child tasks | True project breakdown |

---

## 3. How the Ordering Algorithm Could Evolve

### Current Limitations

The current formula treats all tasks within a deadline band as equal — differentiated only by whether they're in a project/area and by their creation order. It cannot distinguish:

- A 5-minute task from a 3-hour task
- A "must do or the business breaks" task from a "nice to have" task
- A task the user has energy for right now vs. one requiring deep focus

### Evolution Path: Three Levels

#### Level 1 — Add Priority (Quick Win)

Add a `priority: Priority` field (High / Medium / Low, default Medium).

**New sort key:**
```
0. Blocked → bottom
1. Deadline urgency → sooner first
2. Priority → High before Medium before Low
3. Project grouping
4. Area grouping
5. Task number
```

This is the smallest meaningful change. It gives users a manual lever without requiring any algorithmic complexity.

**Impact:** Users can finally distinguish "must do today" from "would be nice." Combined with the existing deadline system, this covers 80% of urgency signaling.

#### Level 2 — Add Time Awareness (Capacity Planning)

Add an optional `estimate: Option<Duration>` field.

**New derived concept: Available Time Budget**

```
Today's budget = sum of estimates for today's tasks
```

Displayed as a header line in `tdo view today`:
```
Today (Feb 22)                       4 tasks · ~3h 30m estimated
```

The sort order doesn't change much, but the system surfaces load information. Users know before they start whether today is over-planned.

**Secondary feature:** `tdo start <id>` / `tdo stop <id>` to track actuals.
**Report:** `tdo stats today` → estimated vs. actual.

#### Level 3 — Energy-Aware Ordering (Context Sensitivity)

The most ambitious evolution. Add an `energy: Energy` field (High / Medium / Low) representing the cognitive load a task requires.

**New compound sort signal:**

Rather than a strict ladder, compute a **composite urgency score** per task:

```
urgency_score =
    deadline_score (days until deadline, inverted)
  + priority_score (High=3, Med=2, Low=1)
  - energy_penalty (optional: deprioritize high-energy tasks after 3pm)
```

Then sort descending by `urgency_score` within each group.

This is analogous to how tools like Superproductivity or linear-algebra GTD systems work. It's a genuine "formula" in the mathematical sense.

**Risks:**
- Computed scores are less predictable — users may not understand why task order changes
- Requires time-of-day awareness (which tdo deliberately avoids today)
- Adds real complexity to the data model

**Recommendation:** Only pursue Level 3 after Level 1 and 2 are stable. The predictability of the current strict sort is a feature; erode it carefully.

---

## 4. Recommended Roadmap

### Milestone A: Usability Floor (next sprint)

Complete the features that make tdo usable at scale. Without search and filters, the tool degrades badly past 50 tasks.

1. **Search command** — `tdo search <query>` (title + notes)
2. **Filter flags** — `--project`, `--area`, `--tag` on all view commands
3. **Fuzzy match confirmation** — prompt before destructive fuzzy ops
4. **Defer until CLI** — expose `--defer-until` flag (model already exists)

### Milestone B: Priority Signal (following sprint)

1. **Priority field** — `--priority high/med/low`, color coded in UI
2. **Dependency CLI flags** — `--depends-on <id>`, `--blocks <id>`
3. **Checklist CLI** — `tdo checklist <id> check <n>`, progress indicator in list

### Milestone C: Power User Layer (quarter)

1. **Recurring tasks** — `--every monday/daily/"1st of month"`
2. **Time estimates** — `--estimate 2h`, shown in today's header
3. **Enhanced date parser** — `+3d`, `eom`, `eoq`, `q2`
4. **Import** — CSV and Todoist JSON

### Milestone D: Ecosystem (half-year)

1. **Sync** — expose saku-sync to end users
2. **Config file** — color theming, default project, editor preferences
3. **TUI mode** — keyboard-driven interactive view
4. **Subtasks** — `--parent <id>`, max 2 levels deep

---

## 5. Formula Evolution Summary

| Phase | Ordering Formula | New Fields Required |
|---|---|---|
| Today (v0.5.11) | Blocked → Deadline → Project → Area → Task# | — |
| Level 1 | Blocked → Deadline → **Priority** → Project → Area → Task# | `priority: Priority` |
| Level 2 | Level 1 + **estimate display in header** | `estimate: Duration`, `time_entries` |
| Level 3 | **Composite urgency score** (deadline + priority − energy) | `energy: Energy` |

The jump from the current formula to Level 1 is the highest-ROI move: one new field, predictable behavior, immediate user value.

---

## 6. Competitive Position After Roadmap Completion

Assuming milestones A–C are completed:

| Feature | TDO (projected) | Taskwarrior | Things 3 | Todoist |
|---|---|---|---|---|
| CLI-first | ✅ | ✅ | ❌ | ❌ |
| Natural language dates | ✅ | ✅ | ✅ | ✅ |
| Projects/Areas | ✅ | ✅ | ✅ | ✅ |
| Search | ✅ | ✅ | ✅ | ✅ |
| Priorities | ✅ | ✅ | ❌ | ✅ |
| Recurring tasks | ✅ | ✅ | ✅ | ✅ |
| Task dependencies | ✅ | ✅ | ❌ | ❌ |
| Time tracking | ✅ | ❌ | ❌ | ❌ |
| Bulk operations | ✅ | ✅ | ✅ | ✅ |
| Export/Import | ✅ | ✅ | ✅ | ✅ |
| Mobile/Web | ❌ | ❌ | ✅ | ✅ |
| Open source | ✅ | ✅ | ❌ | ❌ |

At that point, tdo would be the most feature-complete open-source, CLI-first task manager — surpassing Taskwarrior's usability while remaining developer-friendly.

---

*This report supersedes the v0.4.0 QA review status sections. See `qa-review-report.md` for the original issue catalogue with updated resolution status.*
