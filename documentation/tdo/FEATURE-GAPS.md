# Feature Gaps - TDO CLI

**Priority:** P2 (Medium - Nice to Have)  
**Estimated Total Effort:** 2-3 months  

---

## Task 1: Recurring Tasks

**Problem:** No way to create tasks that repeat on a schedule

**Use Cases:**
- Weekly team meetings
- Monthly bill payments
- Daily habits/routines
- Quarterly reviews

**Expected Behavior:**
```bash
# Add recurring task
tdo add "Team standup" --every monday --project work

# Add with end date
tdo add "Gym session" --every "mon,wed,fri" --until 2026-12-31

# Monthly recurring
tdo add "Pay rent" --every "1st of month" --project personal

# Complex patterns
tdo add "Review KPIs" --every "1st monday of month" --project work
```

**Features:**
- Basic frequencies: daily, weekly, monthly, yearly
- Specific days: "monday", "mon,wed,fri"
- Date-based: "1st of month", "last friday"
- End date support: `--until <date>`
- Auto-create next instance when current is completed
- View recurring task template and all instances

**Implementation Details:**
- Add `recurrence` field to Task model (stores pattern)
- Add `recurring_template_id` field (links instances to template)
- When task completed, generate next instance based on pattern
- Add `tdo view recurring` to see all recurring templates
- Need date calculation library for complex patterns

**Files to Change:**
- `crates/tdo/src/models/task.rs` - add recurrence fields
- `crates/tdo/src/services/tasks.rs` - recurrence logic
- Storage migration (v5 or v6)
- Tests: verify recurrence generation

**Estimated Effort:** 20-25 hours

**Steps:**
- [ ] Design recurrence data model
- [ ] Add recurrence fields to Task
- [ ] Implement recurrence pattern parser
- [ ] Add `--every` and `--until` flags
- [ ] Implement next instance generation logic
- [ ] Add `tdo view recurring` command
- [ ] Handle edge cases (holidays, weekends)
- [ ] Add tests for all recurrence patterns
- [ ] Update documentation with examples
- [ ] Consider UI for editing recurrence patterns

---

## Task 2: Task Dependencies (Blocks/Depends-On) ✅ SHIPPED

**Status:** Done — shipped in commit `dd713f6`

**What was delivered:**
- `depends_on: Vec<Uuid>` field added to the Task model
- Blocked tasks are deprioritized in all views (sort to bottom of lists)
- `store.is_task_blocked()` method checks all dependencies
- `order_tasks_with_store()` surfaces blocked status in sorting algorithm

**Remaining (deferred to Phase 3):**
- `--blocks` / `--depends-on` CLI flags (currently only settable via `tdo edit`)
- `tdo view today --ready` filter
- Visual dependency indicators in task list rendering
- Dependency graph visualization

**Steps:**
- [x] Add dependency fields to Task model
- [ ] Add `--blocks` and `--depends-on` flags
- [x] Implement circular dependency detection (blocked check in store)
- [x] Add validation when creating/deleting tasks
- [ ] Display dependencies in task views
- [ ] Add `--ready` filter to views
- [ ] Handle task deletion (break dependencies or cascade?)
- [x] Add tests for dependency scenarios
- [ ] Update documentation
- [ ] Consider: dependency graph visualization

---

## Task 3: Enhanced Checklist Functionality

**Problem:** Checklists exist but are underutilized - not accessible via CLI

**Current State:**
- Checklists only editable via `tdo edit <id>` (opens editor)
- No CLI commands to manage checklist items
- No visual progress indicator

**Expected Behavior:**
```bash
# Add task with checklist
tdo add "Deploy backend" --checklist "Run tests,Update docs,Deploy staging,Deploy prod"

# Manage checklist items
tdo checklist 42 add "Notify team"
tdo checklist 42 check 1              # Mark item 1 as done
tdo checklist 42 uncheck 2            # Unmark item 2
tdo checklist 42 remove 3             # Delete item 3

# View task with checklist progress
tdo view today
  42  ○  Deploy backend (2/4 done)    Work / api-project
```

**Features:**
- Add checklist items via `--checklist` flag (comma-separated)
- Dedicated `tdo checklist` commands for CRUD operations
- Show progress in task display: "(2/5 done)"
- Progress bar option: "▮▮▮▯▯ 60%"
- Filter: `tdo view today --incomplete-checklists`

**Implementation Details:**
- Checklist field already exists in model
- Add CLI commands to manipulate checklists
- Update UI to show progress
- Add checklist-specific view filters

**Files to Change:**
- `crates/tdo/src/main.rs` - add checklist subcommands
- `crates/tdo/src/services/` - add checklist service
- `crates/tdo/src/ui.rs` - show checklist progress
- Tests: checklist operations

**Estimated Effort:** 10-12 hours

**Steps:**
- [ ] Add `--checklist` flag to add/move commands
- [ ] Add `tdo checklist` subcommands (add, check, remove)
- [ ] Parse comma-separated checklist items
- [ ] Update UI to show "(X/Y done)" progress
- [ ] Add optional progress bar visualization
- [ ] Add filter for incomplete checklists
- [ ] Add tests for all checklist operations
- [ ] Update documentation

---

## Task 4: Priority System

**Problem:** No way to distinguish urgent from non-urgent tasks

**Current Workarounds:**
- Use tags: `-t urgent` (no special treatment)
- Use deadlines (but that's date-based, not priority)

**Expected Behavior:**
```bash
# Add task with priority
tdo add "Critical bug" --priority high --project work

# Change priority
tdo move 42 --priority low

# View by priority
tdo view today --priority high        # Only high priority
tdo view today --sort priority        # Sort by priority first

# Display with priority indicators
tdo view today
  42  ●  Critical bug [HIGH]           Work / api-project    ⚑ Feb 20
  51  ○  Review PR [MED]               Work / api-project
  67  ○  Update docs                   Work / api-project
```

**Features:**
- Priority levels: `high`, `medium`, `low` (default: medium)
- Color coding: red (high), yellow (medium), default (low)
- Sort by priority in views
- Filter by priority
- Priority badges: `[HIGH]`, `[MED]`, `[LOW]`

**Implementation Details:**
- Add `priority` enum to Task model (High, Medium, Low)
- Add `--priority` flag to add/move commands
- Update task ordering to consider priority
- Add color coding in UI
- Add priority filters to views

**Files to Change:**
- `crates/tdo/src/models/task.rs` - add Priority enum
- `crates/tdo/src/services/tasks.rs` - handle priority
- `crates/tdo/src/ui.rs` - display priority
- Storage migration
- Tests: priority sorting and filtering

**Estimated Effort:** 8-10 hours

**Steps:**
- [ ] Add Priority enum (High, Medium, Low)
- [ ] Add `priority` field to Task model
- [ ] Add `--priority` flag to CLI commands
- [ ] Update task ordering algorithm
- [ ] Add priority badges to UI
- [ ] Add color coding for priority levels
- [ ] Add `--priority` and `--sort priority` filters
- [ ] Add storage migration
- [ ] Add tests
- [ ] Update documentation

---

## Task 5: Implement Defer Until Feature

**Problem:** `defer_until` field exists but is hidden - not accessible via CLI

**Current State:**
- Field exists in model but not exposed
- Only editable via interactive editor
- Tasks don't auto-appear when defer date arrives

**Expected Behavior:**
```bash
# Defer task
tdo add "Follow up with client" --defer-until 2026-03-01
tdo move 42 --defer-until "next week"

# View deferred tasks
tdo view deferred

DEFERRED TASKS (3)

Next Week (2)
  42  ○  Follow up with client        Business  | Deferred until Mar 1
  51  ○  Review contract               Business  | Deferred until Mar 3

Later (1)
  67  ○  Plan Q2 goals                 Work      | Deferred until Apr 1
```

**Features:**
- Add `--defer-until <date>` flag
- Deferred tasks hidden from normal views until date arrives
- Dedicated `tdo view deferred` command
- On defer date, task appears in inbox automatically
- Clear defer: `tdo move 42 --clear-defer`

**Implementation Details:**
- Field already exists, just need CLI exposure
- Filter deferred tasks from views (check defer_until > today)
- Add logic to auto-show when date arrives
- Add dedicated deferred view

**Files to Change:**
- `crates/tdo/src/main.rs` - add `--defer-until` flag
- `crates/tdo/src/services/tasks.rs` - defer logic
- `crates/tdo/src/models/store.rs` - filter deferred tasks
- Tests: defer filtering

**Estimated Effort:** 5-6 hours

**Steps:**
- [ ] Add `--defer-until` flag to add/move commands
- [ ] Add `--clear-defer` flag
- [ ] Filter deferred tasks from standard views
- [ ] Add `tdo view deferred` command
- [ ] Show defer date in task display
- [ ] Add tests for defer behavior
- [ ] Update documentation

---

## Task 6: Enhanced Date Parsing

**Problem:** Date parser is basic, missing useful shortcuts

**Current Support:**
- `today`, `tomorrow`, `monday`, `next week`, `2026-03-15`

**Missing Shortcuts:**
```bash
# Relative dates
--when "+3d"                    # 3 days from now
--when "+2w"                    # 2 weeks from now
--deadline "+1m"                # 1 month from now

# Smart keywords
--when "eom"                    # End of month
--when "eoq"                    # End of quarter
--when "eoy"                    # End of year
--when "q2"                     # Start of Q2

# Advanced relative
--when "in 3 days"
--when "in 2 weeks"
--when "in 1 month"
```

**Implementation Details:**
- Extend `date_parser.rs` with new patterns
- Add regex patterns for relative dates (+3d, +2w, etc.)
- Add keyword dictionary (eom, eoq, eoy, q1-q4)
- Calculate dates based on current time

**Files to Change:**
- `crates/tdo/src/date_parser.rs` - add new patterns
- Tests: verify all date formats

**Estimated Effort:** 6-8 hours

**Steps:**
- [ ] Add relative date syntax (+Nd, +Nw, +Nm)
- [ ] Add "in X days/weeks/months" parsing
- [ ] Add end-of-period keywords (eom, eoq, eoy)
- [ ] Add quarter keywords (q1, q2, q3, q4)
- [ ] Handle edge cases (leap years, month boundaries)
- [ ] Add comprehensive tests
- [ ] Update documentation with examples

---

## Task 7: Time Estimates and Tracking

**Problem:** No way to track time spent on tasks or estimate duration

**Use Cases:**
- Capacity planning (do I have time today?)
- Learning from estimates (was I right?)
- Time budgets per project
- Billing/reporting

**Expected Behavior:**
```bash
# Add task with estimate
tdo add "Write API docs" --estimate 2h --project work

# Start tracking time
tdo start 42
⏱ Started tracking: Write API docs

# Stop tracking
tdo stop 42
⏱ Stopped tracking: 1h 23m (estimate: 2h)

# View time summary
tdo stats today
Today's Time Summary:
  Estimated: 6h
  Actual:    4h 23m
  Remaining: 1h 37m

# View by project
tdo stats --project work
Project: work
  Total time: 23h 45m
  Avg per task: 2h 22m
  Tasks completed: 10
```

**Features:**
- `--estimate <duration>` flag (1h, 30m, 2h30m)
- `tdo start <id>` / `tdo stop <id>` for time tracking
- Show estimate vs actual on completion
- Summary views: daily, weekly, by project
- Capacity planning: "Today: 3h estimated, 2h remaining"

**Implementation Details:**
- Add `estimate` and `time_entries` fields to Task
- Add time tracking commands
- Store start/stop timestamps
- Calculate total time per task
- Add stats/reporting commands

**Files to Change:**
- `crates/tdo/src/models/task.rs` - add time fields
- `crates/tdo/src/services/` - add time tracking service
- `crates/tdo/src/main.rs` - add start/stop/stats commands
- Storage migration
- Tests: time tracking logic

**Estimated Effort:** 20-25 hours

**Steps:**
- [ ] Add estimate and time_entries fields
- [ ] Add `--estimate` flag
- [ ] Implement `tdo start` command
- [ ] Implement `tdo stop` command
- [ ] Handle multiple tracking sessions per task
- [ ] Implement `tdo stats` command
- [ ] Add time summaries by day/week/project
- [ ] Show estimate vs actual on completion
- [ ] Add tests for time calculations
- [ ] Update documentation

---

## Task 8: Export and Import

**Problem:** Data is locked in JSON format, no interoperability with other tools

**Expected Behavior:**
```bash
# Export to different formats
tdo export --format csv > tasks.csv
tdo export --format markdown > tasks.md
tdo export --format json > backup.json

# Export with filters
tdo export --format csv --project work > work-tasks.csv
tdo export --format md --area personal --completed-after 2026-01-01

# Import from other tools
tdo import --from todoist export.json
tdo import --from csv tasks.csv
tdo import --from things backup.json
```

**Formats to Support:**
- CSV (spreadsheets)
- Markdown (documentation, Obsidian)
- JSON (backup, scripting)
- iCal (calendar integration for deadlines)

**Import Sources:**
- Todoist
- Things 3
- Taskwarrior
- CSV (generic)

**Implementation Details:**
- Add export/import subcommands
- Implement formatters for each format
- Add filters to export (project, area, date range)
- Implement parsers for import formats
- Handle ID conflicts on import

**Files to Change:**
- `crates/tdo/src/main.rs` - add export/import commands
- `crates/tdo/src/services/export.rs` - new service
- `crates/tdo/src/services/import.rs` - new service
- Tests: export/import round-trips

**Estimated Effort:** 15-20 hours

**Steps:**
- [ ] Design export command structure
- [ ] Implement CSV exporter
- [ ] Implement Markdown exporter
- [ ] Implement iCal exporter for deadlines
- [ ] Implement JSON importer
- [ ] Implement CSV importer
- [ ] Add import from Todoist
- [ ] Add import from Things (if format available)
- [ ] Handle ID conflicts on import
- [ ] Add tests for all formats
- [ ] Update documentation with examples

---

## Task 9: Subtasks / Task Hierarchy

**Problem:** Checklists are flat, can't represent true parent-child task relationships

**Use Cases:**
- Break down large tasks into smaller ones
- Project milestones with sub-tasks
- Multi-step processes

**Expected Behavior:**
```bash
# Create parent task
tdo add "Launch new feature" --project work

# Create subtasks
tdo add "Write API endpoints" --parent 42
tdo add "Write frontend" --parent 42
tdo add "Write tests" --parent 42

# View with hierarchy
tdo view today
  42  ○  Launch new feature (1/3)     Work / api-project
      ├─ 43 ✓ Write API endpoints
      ├─ 44 ○ Write frontend
      └─ 45 ○ Write tests

# Auto-complete parent when all children done
tdo done 44
tdo done 45
✓ Completed task #45: Write tests
✓ Auto-completed parent #42: Launch new feature (all subtasks done)
```

**Features:**
- `--parent <id>` flag to create subtask
- Visual tree display with Unicode box chars
- Progress indicator on parent: "(2/3 done)"
- Auto-complete parent when all children complete
- Max depth: 2 or 3 levels (avoid deep nesting)

**Implementation Details:**
- Add `parent_task_id` field to Task
- Query methods for children
- Recursive completion logic
- Tree rendering in UI

**Files to Change:**
- `crates/tdo/src/models/task.rs` - add parent field
- `crates/tdo/src/services/tasks.rs` - subtask logic
- `crates/tdo/src/ui.rs` - tree rendering
- Storage migration
- Tests: hierarchy operations

**Estimated Effort:** 15-18 hours

**Steps:**
- [ ] Add `parent_task_id` field
- [ ] Add `--parent` flag to add command
- [ ] Implement child query methods
- [ ] Add tree rendering with Unicode chars
- [ ] Show progress on parent tasks
- [ ] Implement auto-complete parent logic
- [ ] Limit nesting depth (prevent infinite recursion)
- [ ] Handle parent deletion (orphan children?)
- [ ] Add tests for hierarchy operations
- [ ] Update documentation

---

## Priority Ranking (Suggested Order)

1. **Priority system** (10 hours) - Simple, high impact
2. **Defer until** (6 hours) - Field exists, just needs exposure
3. **Enhanced date parsing** (8 hours) - Improves existing feature
4. **Checklist enhancements** (12 hours) - Builds on existing feature
5. **Recurring tasks** (25 hours) - Highly requested, complex
6. **Dependencies** (20 hours) - Power user feature
7. **Time tracking** (25 hours) - Nice to have, complex
8. **Export/import** (20 hours) - Important for data portability
9. **Subtasks** (18 hours) - Advanced feature, lower priority

**Total Effort:** 144 hours (~3.5 weeks full-time)

---

## Quick Wins (Do First)

1. **Defer until** - Field exists, just needs CLI exposure
2. **Priority system** - Simple enum, straightforward implementation
3. **Enhanced date parsing** - Extends existing parser

These three can be done in ~24 hours and provide immediate value.

---

## Long-term Features (Phase 2+)

These are good ideas but lower priority:
- Subtasks (complex, checklist may be sufficient)
- Time tracking (niche use case)
- Import from other tools (one-time need)

---

## Testing Requirements

Each feature must include:
- [ ] Unit tests for core logic
- [ ] Integration tests for CLI
- [ ] Edge case testing
- [ ] Documentation with examples
- [ ] Migration tests (if schema changes)
