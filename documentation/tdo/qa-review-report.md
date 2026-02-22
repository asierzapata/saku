# TDO CLI - Comprehensive QA & Product Review Report

**Report Date:** February 19, 2026 (updated February 22, 2026)
**Reviewer:** QA Engineer & Product Manager Review
**Version Analyzed:** v0.4.0 (status updated to v0.5.11)
**Current Version:** v0.5.11

---

## Executive Summary

**TDO is a well-architected, production-ready task management CLI** with clean code, strong fundamentals, and thoughtful design. However, there are **significant UX inconsistencies**, **missing features**, and **underdeveloped capabilities** that limit its usability and competitive position.

**Overall Grade (v0.4.0):** B+ (Code Quality) / C+ (Feature Completeness & UX)
**Updated Grade (v0.5.11):** B+ (Code Quality) / B- (Feature Completeness & UX)

---

## 🎯 Critical Issues (P0 - Must Fix)

### 1. **Outdated Documentation - "Anytime" Feature** ❌

**Problem:** 
- Multiple documentation files mention `tdo anytime` command and `--anytime` flag
- The code has `LegacyAnytime` in the When enum (migration support only)
- **"Anytime" was removed from the feature set but docs weren't updated**

**Affected documentation files:**
- `/crates/tdo/README.md` - line 7, 27
- `/documentation/tdo/commands-cheat-sheet.md` - lines 11, 19, 29, 67, 78, 110
- `/documentation/tdo/testing.md` - lines 34, 35
- `/crates/tdo/documentation/architecture.md` - line 175
- `/skills/saku-integration/SKILL.md` - line 36

**Evidence:**
```rust
// From models/task.rs
pub enum When {
    Inbox,
    Someday,
    Scheduled { date: Date, evening: Option<bool> },
    LegacyToday { evening: bool },     // Migration only
    LegacyAnytime,  // ← Migration only - can't create new ones!
}
```

**Impact:** Users will be confused by documentation that doesn't match CLI behavior.

**Recommendation:** 
- ✅ Remove all "Anytime" references from documentation
- Update examples to use: Inbox, Today (scheduled), Someday, or Upcoming (specific dates)
- Note: This issue has been addressed in this review

---

### 2. **Command Structure Inconsistency** ⚠️

**Problem:** The CLI has mixed command patterns:
- `tdo today` (direct command)
- `tdo create area "Name"` (nested subcommand)
- `tdo list areas` (nested subcommand)

**Examples:**
```bash
# Direct view commands:
tdo today
tdo inbox
tdo someday
tdo upcoming
tdo logbook

# Nested entity commands:
tdo create area "Name"
tdo list areas
tdo show project "name"
```

**Impact:** Slight learning curve - users need to know which pattern applies where.

**Recommendation:** 
- Consider flattening: `tdo areas`, `tdo projects`, `tdo tags` instead of `tdo list areas`
- Or nest all views: `tdo view today`, `tdo view inbox`, etc.
- Current structure is acceptable but could be more consistent

---

### 3. **Outdated Documentation - "Evening" Feature** ❌

**Problem:**
- Code has `evening: Option<bool>` field in `When::Scheduled`
- Documentation shows `--evening` flag
- **"Evening" was deprecated but references remain in code and docs**
- Only exists for legacy migration support (`LegacyToday { evening: bool }`)

**Affected locations:**
- `/documentation/tdo/commands-cheat-sheet.md` - lines 9, 65, 108
- `/documentation/tdo/design-spec.md` - line 98 (mockup example)
- Code: `models/task.rs` - `When::Scheduled { evening: Option<bool> }`

**Impact:** Confusing data model with unused field, misleading documentation.

**Recommendation:**
- ✅ Remove all "evening" references from documentation
- ✅ Remove `evening` field from `When::Scheduled` (requires migration v5)
- Keep `LegacyToday { evening: bool }` for backward compatibility only
- Note: This cleanup is recommended in this review

---

## 🚨 Major UX Issues (P1)

### 4. **No Bulk Operations** ❌

**Problem:** Can't operate on multiple tasks at once.

**Missing capabilities:**
```bash
# These would be super useful but don't exist:
tdo done 1 2 3 4           # Complete multiple
tdo move 1,2,3 --today     # Reschedule multiple
tdo delete 5-10            # Delete range
tdo tag 1,2,3 urgent       # Bulk tagging
```

**Impact:** Tedious workflow for common operations.

**Recommendation:** Add bulk operation support with:
- Comma-separated IDs: `1,2,3`
- Range syntax: `1-5`
- Wildcard/filter: `tdo done @project-name` (complete all in project)

---

### 5. **Fuzzy Matching is Dangerous** ⚠️

**Problem:** The fuzzy matching for `tdo done <name>` and `tdo delete <name>` can accidentally match the wrong task.

**Current behavior:**
```rust
// From services/tasks.rs - uses case-insensitive substring match
let matching: Vec<_> = store
    .get_active_tasks()
    .filter(|t| t.title.to_lowercase().contains(&identifier.to_lowercase()))
    .collect();
    
match matching.len() {
    0 => Err(TaskNotFound),
    1 => complete_task(matching[0]),  // ← Completes without confirmation!
    _ => Err(AmbiguousTaskName),
}
```

**Example issue:**
```bash
tdo add "Buy milk"
tdo add "Buy milk for recipe"
tdo done "milk"  # ← Error: Ambiguous (good!)

tdo add "Review PR"
tdo done "review"  # ← Completes without asking! (dangerous!)
```

**Impact:** Accidental completions/deletions, especially for destructive operations.

**Recommendation:**
- Add confirmation prompt for fuzzy matches: `Complete "Review PR"? [y/N]`
- Show the matched task before acting
- Require `--force` flag for non-interactive fuzzy operations

---

### 6. **No Search or Filter Capabilities** ❌

**Problem:** No way to search tasks by text, filter by criteria, or find specific items.

**Missing commands:**
```bash
tdo search "keyword"           # Search in titles/notes
tdo find --area work --overdue # Complex filters
tdo filter --tag urgent --project alpha
```

**Workarounds:** Users must pipe to `grep`:
```bash
tdo all | grep "keyword"  # Loses formatting, not user-friendly
```

**Impact:** Unusable for large task lists (100+ tasks).

**Recommendation:** Add:
- `tdo search <query>` - search titles and notes
- Filter flags on view commands: `tdo today --project work --tag urgent`

---

### 7. **Poor Deadline Visibility** ⚠️

**Problem:** Deadlines are secondary in the UI:
- Only shown as small badges on task lines
- No dedicated "Deadlines" view
- No warning for upcoming deadlines
- Overdue detection only in Today view

**Impact:** Users will miss deadlines.

**Recommendation:**
- Add `tdo deadlines` command showing all tasks with deadlines (sorted by date)
- Add `--deadline-soon` filter (e.g., next 3 days)
- Make deadline badges more prominent (red/bold when < 3 days)

---

### 8. **No Recurring Tasks** ❌

**Problem:** Can't create recurring/repeating tasks.

**Common use cases:**
```bash
# Would be useful:
tdo add "Weekly team meeting" --every monday
tdo add "Pay rent" --every "1st of month"
tdo add "Exercise" --every day --project health
```

**Impact:** Users must manually recreate tasks weekly/monthly.

**Recommendation:** 
- Add recurrence support in Task model
- Add `--repeat` / `--every` flags
- Auto-create next instance when current is completed

---

## 🔧 Feature Gaps (P2)

### 9. **Checklist Functionality Underutilized** ⚠️

**Problem:** Tasks have a `checklist` field but:
- Not accessible via CLI flags
- Only editable via `tdo edit <id>` (opens editor)
- No way to mark individual checklist items as done
- No visual progress indicator (e.g., "3/5 subtasks")

**Recommendation:**
- Add `--checklist "item1,item2,item3"` flag to `add`/`move`
- Add `tdo checklist <task-id> check <item-number>` command
- Show progress in task display: `▢ Task title (2/5 done)`

---

### 10. **No Task Dependencies** ❌

**Problem:** Can't link tasks or create dependencies.

**Use cases:**
```bash
# Useful relationships:
tdo add "Deploy backend" --blocks 42    # Can't do #42 until this is done
tdo add "Write tests" --depends-on 15   # Must do #15 first
tdo add "Bug fix" --related 7,8,9       # Related tasks
```

**Impact:** No project management capabilities.

**Recommendation:** Add:
- `--blocks <id>` / `--depends-on <id>` flags
- Show dependencies in task display
- Filter: `tdo today --ready` (no unsatisfied dependencies)

---

### 11. **Limited Date Intelligence** ⚠️

**Problem:** Date parsing is basic. Missing useful shortcuts:

**Current support:** `today`, `tomorrow`, `monday`, `next week`, `2026-03-15`

**Missing:**
```bash
--when "in 3 days"
--when "in 2 weeks"
--when "end of month"
--when "next quarter"
--deadline "+7d"  # Relative dates
```

**Recommendation:** Enhance date_parser.rs with:
- Relative date syntax: `+3d`, `+2w`, `+1m`
- Smart keywords: `eom` (end of month), `eoy`, `q1`, `q2`

---

### 12. **Defer Until Feature Hidden** ⚠️

**Problem:** Tasks have a `defer_until` field but it's:
- Not accessible via CLI flags
- Only editable in interactive editor
- No visual indication when a task is deferred
- No automatic appearance when defer date arrives

**Impact:** Feature exists but is unusable.

**Recommendation:**
- Add `--defer-until <date>` flag
- Filter out deferred tasks from views until their date
- Add `tdo deferred` view to see all deferred tasks

---

### 13. **No Prioritization System** ❌

**Problem:** No way to mark tasks as urgent/important.

**Current workarounds:**
- Use tags: `-t urgent` (but no special treatment)
- Use deadlines (but that's date-based, not priority)

**Impact:** Can't distinguish "must do now" from "nice to have."

**Recommendation:** Add:
- Priority field: `--priority high/medium/low`
- Sort by priority in views
- Color coding: red (high), yellow (medium), default (low)

---

### 14. **No Time Estimates or Tracking** ❌

**Problem:** Can't track:
- How long tasks will take (estimates)
- How long tasks took (actual time)
- Time budgets per project/area

**Impact:** No capacity planning, can't learn from estimates.

**Recommendation:** Add:
- `--estimate 2h` flag
- `tdo start <id>` / `tdo stop <id>` for time tracking
- Summary: `tdo stats --project work` (total time, avg per task)

---

## 🎨 UI/UX Improvements (P2)

### 15. **Inconsistent Output Formatting** ⚠️

**Problem:** Looking at the code, there are inconsistencies:

**Task rendering:**
```rust
// From ui.rs - tasks use custom rendering
render_task_line(task, &store);

// But projects/areas use basic println!
println!("  {} {}", "•".dimmed(), project.name.dimmed());
```

**Impact:** Visual inconsistency between entity types.

**Recommendation:**
- Create `render_project_line()` and `render_area_line()` for consistency
- Use same visual style (bullets, spacing, colors) across all entities

---

### 16. **No Interactive Mode** ❌

**Problem:** Every operation requires a full command. No TUI/interactive mode.

**Competitor comparison:**
- `taskwarrior`: Has interactive reports
- `todoist`: TUI with arrow key navigation
- `todo.txt`: CLI-only (like tdo)

**Missing experience:**
```bash
tdo interactive  # Would be nice:
# - Arrow keys to navigate
# - Space to mark done
# - d to delete
# - e to edit
# - Vim-like keybindings
```

**Impact:** Less efficient for power users.

**Recommendation:** Add optional TUI mode (future enhancement).

---

### 17. **No Color Customization** ⚠️

**Problem:** Colors are hardcoded:
```rust
// From ui.rs
.red()     // Overdue
.yellow()  // Approaching deadline
.green()   // On track
.dimmed()  // Context
```

**Impact:** Not accessible for colorblind users, no theming.

**Recommendation:**
- Add config file: `~/.config/tdo/config.toml`
- Allow color customization per urgency level
- Add `--no-color` flag for plain output

---

### 18. **Limited Export/Import** ❌

**Problem:** Data is locked in JSON format. No way to:
- Export to CSV, Markdown, or other formats
- Import from other task managers
- Generate reports

**Missing commands:**
```bash
tdo export --format csv > tasks.csv
tdo export --format markdown --project work > work.md
tdo import --from todoist tasks.json
tdo report --weekly  # Week summary
```

**Impact:** Data lock-in, no portability.

**Recommendation:** Add export/import subsystem:
- `tdo export [--format csv|json|md] [--filter]`
- `tdo import [--from format] <file>`

---

## 🐛 Code Quality Issues

### 19. **Dead Code Warning** ⚠️

**Finding:**
```
warning: function `format_completion_date` is never used
   --> crates/tdo/src/ui.rs:276:4
```

**Impact:** Code smell, indicates incomplete feature.

**Recommendation:** 
- Either use the function (probably for logbook view improvements)
- Or remove it

---

### 20. **Clippy Warnings** ⚠️

**Finding:** Several clippy warnings:
- `io_other_error` - use `std::io::Error::other()`
- Collapsible if statements
- Manual `RangeInclusive::contains` implementation

**Impact:** Code quality, potential bugs.

**Recommendation:** Run `cargo clippy --fix` and address all warnings.

---

### 21. **Test Coverage Gaps** ⚠️

**Finding:** Integration tests exist but don't cover:
- Error paths (ambiguous names, invalid dates)
- Edge cases (empty stores, concurrent access)
- Migration paths (v1→v2→v3→v4)

**Recommendation:**
- Add negative test cases
- Add concurrent access tests (file locking)
- Add migration tests with real v1/v2/v3 JSON files

---

## 🎯 Strategic Product Gaps

### 22. **No Mobile/Web Companion** ❌

**Problem:** CLI-only limits usage scenarios:
- Can't add tasks from phone
- Can't check tasks in meetings
- No sync across devices (sync crate exists but not integrated)

**Impact:** Limited adoption outside power users.

**Recommendation:** 
- Implement sync (saku-sync exists but not exposed to users)
- Add web UI or mobile app (long-term)
- Or integrate with existing tools (Obsidian, Notion, etc.)

---

### 23. **No Integrations** ❌

**Problem:** Isolated tool with no hooks/plugins:

**Missing integrations:**
- Git hooks (add tasks from commit messages)
- Calendar sync (export deadlines to iCal)
- Email (send daily digests)
- Webhooks (trigger on task completion)

**Impact:** Doesn't fit into existing workflows.

**Recommendation:**
- Add plugin system or webhook support
- Add `tdo hooks` for git integration
- Add `tdo sync-calendar` for iCal export

---

### 24. **No Documentation for Power Users** ⚠️

**Problem:** Missing advanced docs:
- No keyboard shortcuts reference
- No automation examples (shell scripts)
- No best practices guide
- No migration guide from other tools

**Recommendation:** Add:
- `documentation/tdo/advanced-usage.md`
- `documentation/tdo/migration-guides/` (from Things, Todoist, etc.)
- `documentation/tdo/automation.md` (shell aliases, scripts)

---

## ✅ What's Working Well

### Strengths (Keep These!)

1. **Clean Architecture** ✅
   - Well-separated layers (models, services, storage, UI)
   - Testable design with mock storage
   - Clear error handling

2. **Data Safety** ✅
   - Automatic backups (last 5)
   - File locking prevents corruption
   - Soft deletes with restore capability
   - Schema migrations for upgrades

3. **Fuzzy Matching** ✅
   - User-friendly name resolution for projects/areas
   - Ambiguity detection prevents mistakes

4. **Natural Language Dates** ✅
   - "tomorrow", "monday", "next week" work well
   - Intuitive date parsing

5. **Task Numbering** ✅
   - Stable IDs make referencing easy
   - Auto-incrementing is predictable

6. **Editor Integration** ✅
   - `tdo edit <id>` opens $EDITOR
   - Human-readable format
   - Change detection

7. **Hierarchical Organization** ✅
   - Area → Project → Task is clear
   - Optional associations (not forced)

8. **Performance** ✅
   - Fast startup (<10ms)
   - HashMap-based lookups (O(1))
   - No async overhead

---

## 📊 Comparison with Competitors

| Feature | TDO | Things 3 | Todoist | Taskwarrior |
|---------|-----|----------|---------|-------------|
| CLI-first | ✅ | ❌ | ❌ | ✅ |
| Natural language dates | ✅ | ✅ | ✅ | ✅ |
| Projects/Areas | ✅ | ✅ | ✅ | ✅ |
| Tags | ✅ | ✅ | ✅ | ✅ |
| Recurring tasks | ❌ | ✅ | ✅ | ✅ |
| Search | ❌ | ✅ | ✅ | ✅ |
| Priorities | ❌ | ❌ | ✅ | ✅ |
| Time tracking | ❌ | ❌ | ❌ | ✅ |
| Dependencies | ❌ | ❌ | ❌ | ✅ |
| Bulk operations | ❌ | ✅ | ✅ | ✅ |
| Mobile/Web | ❌ | ✅ | ✅ | ❌ |
| Sync | ⚠️ (partial) | ✅ | ✅ | ⚠️ (manual) |
| Export/Import | ❌ | ✅ | ✅ | ✅ |
| Open source | ✅ | ❌ | ❌ | ✅ |
| Human-readable storage | ✅ | ❌ | ❌ | ✅ |

**Verdict:** TDO has solid foundations but lags in features. It's similar to early Taskwarrior but needs more maturity.

---

## 🚀 Recommended Prioritization

### Phase 1: Fix Broken Basics — COMPLETE ✅
1. ✅ Clean up outdated documentation (remove "anytime" and "evening" references)
2. ✅ Remove `evening` field from `When::Scheduled` in code
3. ❌ Add confirmation for fuzzy match operations (still pending)
4. ✅ Fix dead code warnings (clippy pass in `964f73b`)
5. ✅ Fix clippy warnings (clippy pass in `964f73b`)
6. ✅ Command structure standardized: all views under `tdo view <subcommand>`

### Phase 2: Essential Features — PARTIALLY COMPLETE
7. ❌ Add search command (`tdo search <query>`) — not yet built
8. ❌ Add filter flags to view commands — not yet built
9. ✅ Implement bulk operations — batch mode shipped in `e18b4e5`
10. ✅ Add dedicated deadlines view — shipped in `4da5046` / `1f69b6b`
11. ❌ Improve checklist UX (CLI accessible) — still editor-only

### Phase 3: Power User Features — IN PROGRESS
12. ❌ Recurring tasks — not started
13. ⚠️ Task dependencies — data model shipped (`dd713f6`); CLI flags pending
14. ❌ Priority system — not started
15. ❌ Defer until properly implemented — field exists, CLI flags pending
16. ⚠️ Export/import — JSON + CSV output shipped (`412c5d1`); import not started

### Phase 4: Polish & Ecosystem — NOT STARTED
17. ❌ Interactive TUI mode
18. ❌ Configuration file support
19. ❌ Plugin/hook system
20. ❌ Integrate sync functionality (saku-sync exists but not user-exposed)
21. ❌ Advanced documentation

---

## 💡 Additional Recommendations

### Quick Wins (Low effort, high impact)
- Add `tdo version` command (shows version + storage path)
- Add `tdo stats` (task counts by status/area/project)
- Add `--json` flag to all view commands (for scripting)
- Add shell completion (exists but not documented enough)
- Add `tdo doctor` command (verify storage integrity)

### Documentation Improvements
- Add animated GIF demos to README
- Create video walkthrough (5 min intro)
- Add comparison table with Things/Todoist
- Write "Getting Started" guide (< 100 words)

### Community Building
- Create GitHub Discussions for feature requests
- Add CONTRIBUTING.md with clear guidelines
- Set up GitHub Projects board for roadmap visibility
- Add issue templates (bug, feature request, question)

---

## 🎬 Conclusion

**TDO has excellent fundamentals** - the code is clean, the architecture is solid, and the core task management works well. However, **it's only 60% complete** from a product perspective.

**Critical next steps:**
1. Clean up outdated documentation (remove "anytime" and "evening" references)
2. Remove deprecated code (evening field from When::Scheduled)
3. Add search and filtering (table stakes for task managers)
4. Implement recurring tasks (highly requested feature)
5. Improve bulk operations UX

**Long-term vision:**
TDO could differentiate by being the **"developer-friendly task manager"** - CLI-first, scriptable, git-friendly, with a plugin ecosystem. But it needs feature parity with Taskwarrior to compete seriously.

**Current state:** Good for personal use by developers, but not ready for broader adoption.

---

## 📋 Issue Summary

**Total Issues Found: 22** | Updated status as of v0.5.11

- 🚨 **Critical (P0):** 3 issues → **2 resolved** (docs cleanup ✅, code cleanup ✅) | 1 open (command confirmation UX)
- 🔴 **Major (P1):** 7 issues → **2 resolved** (deadlines view ✅, bulk ops ✅) | 5 open
- 🟡 **Feature Gaps (P2):** 9 issues → **1 partially resolved** (dependencies data model ✅, CLI pending) | 8 open
- ⚪ **Code Quality:** 3 issues → **2 resolved** (clippy warnings ✅) | 1 open (test coverage)

**Resolved since v0.4.0:** Command structure refactored, evening/anytime removed, deadlines view, batch mode, task dependencies (model), JSON/CSV export.
**Remaining to address all open issues:** ~6-8 weeks of focused development.

**Note:** Issues #1-3 are documentation/cleanup tasks that can be completed quickly (see `/documentation/tdo/CLEANUP-ANYTIME-EVENING.md` for detailed instructions).

---

*Report generated by comprehensive code review including architecture analysis, command testing, documentation review, and competitive benchmarking.*
