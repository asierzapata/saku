# UI/UX Improvements - TDO CLI

**Priority:** P2-P3 (Polish - Lower Priority)  
**Estimated Total Effort:** 2-3 weeks  

---

## Task 1: Consistent Output Formatting for All Entity Types

**Problem:** Tasks use custom rendering, but projects/areas use basic `println!` statements

**Current Inconsistency:**
```rust
// Tasks: nice rendering
render_task_line(task, &store);

// Projects/Areas: basic output
println!("  {} {}", "•".dimmed(), project.name.dimmed());
```

**Solution:** Create dedicated rendering functions for all entity types

**Expected Behavior:**
```bash
tdo list projects
PROJECTS (3)

  • work-api                              5 tasks • Area: Work
  • personal-site                         2 tasks • Area: Personal
  • learning-rust                         8 tasks
```

**Implementation Details:**
- Create `render_project_line()` function
- Create `render_area_line()` function
- Create `render_tag_line()` function
- Use consistent spacing, bullets, colors across all entity types
- Align metadata to right side (like tasks)

**Files to Change:**
- `crates/tdo/src/ui.rs` - add entity rendering functions
- `crates/tdo/src/main.rs` - use new render functions
- Tests: visual regression tests (snapshot testing)

**Estimated Effort:** 4-5 hours

**Steps:**
- [ ] Create `render_project_line()` with consistent styling
- [ ] Create `render_area_line()` with consistent styling
- [ ] Create `render_tag_line()` with consistent styling
- [ ] Update all `println!` calls to use render functions
- [ ] Ensure spacing and alignment matches task rendering
- [ ] Test with various terminal widths
- [ ] Update design spec if needed

---

## Task 2: Interactive TUI Mode

**Problem:** Every operation requires typing a full command, no keyboard-driven UI

**Use Cases:**
- Quickly browse and mark tasks as done
- Navigate large task lists efficiently
- Batch operations with keyboard shortcuts

**Expected Behavior:**
```bash
tdo interactive

┌─ TDO Interactive Mode ────────────────────────────────────────┐
│ Today (Feb 19) - 5 tasks                               [?] Help │
├────────────────────────────────────────────────────────────────┤
│                                                                 │
│ > 42  ○  Review PR for auth                 Work / api-project │
│   51  ○  Deploy to staging                  Work / api-project │
│   67  ○  Write documentation                Work / api-project │
│   73  ○  Fix CSS bug                        Work / frontend    │
│   81  ○  Team meeting prep                  Work               │
│                                                                 │
├────────────────────────────────────────────────────────────────┤
│ j/k: navigate | Space: done | d: delete | e: edit | q: quit   │
└────────────────────────────────────────────────────────────────┘
```

**Features:**
- Arrow keys / j/k to navigate
- Space to toggle completion
- d to delete (with confirmation)
- e to edit task
- m to move task (opens move dialog)
- f to filter by project/area/tag
- /: search
- Tab to switch views (Today/Inbox/Upcoming/etc.)
- Vim-like keybindings

**Implementation Details:**
- Use `crossterm` or `ratatui` for TUI
- Implement keyboard event handling
- Render task list with selection cursor
- Add modal dialogs for operations
- Persist changes to storage in real-time

**Files to Change:**
- New module: `crates/tdo/src/interactive.rs`
- `crates/tdo/src/main.rs` - add Interactive command
- Add dependencies: `crossterm` or `ratatui`
- Tests: keyboard event handling

**Estimated Effort:** 30-40 hours

**Steps:**
- [ ] Choose TUI library (crossterm vs ratatui)
- [ ] Design interactive UI layout
- [ ] Implement task list rendering with cursor
- [ ] Add keyboard event handling
- [ ] Implement navigation (j/k/arrows)
- [ ] Implement actions (space, d, e, m)
- [ ] Add search/filter within interactive mode
- [ ] Add view switching (tabs)
- [ ] Add help screen (?)
- [ ] Test on different terminals
- [ ] Update documentation
- [ ] Make optional (feature flag?)

---

## Task 3: Configuration File Support

**Problem:** Colors and formatting are hardcoded, no customization

**Use Cases:**
- Colorblind accessibility
- Personal preference (themes)
- Terminal compatibility
- Disable colors for piping

**Expected Behavior:**
```bash
# Config file: ~/.config/tdo/config.toml
[colors]
overdue = "red"
approaching = "yellow"
on_track = "green"
context = "bright-black"

[ui]
date_format = "relative"  # or "iso", "short"
show_task_numbers = true
compact_mode = false

[behavior]
confirm_fuzzy_matches = true
default_view = "today"
```

**Features:**
- Color customization per urgency level
- Date format preferences
- UI density (compact vs spacious)
- Behavior preferences (confirmations, defaults)
- `--no-color` flag to disable all colors
- Respect `NO_COLOR` environment variable

**Implementation Details:**
- Create config parsing logic
- Load config on startup
- Apply config to UI rendering
- Add CLI flag to override config
- Generate default config: `tdo config init`

**Files to Change:**
- New module: `crates/tdo/src/config.rs`
- `crates/tdo/src/ui.rs` - use config for colors
- `crates/tdo/src/main.rs` - load config
- Add dependency: `toml` or `serde_toml`
- Tests: config parsing

**Estimated Effort:** 8-10 hours

**Steps:**
- [ ] Design config schema (TOML)
- [ ] Add config parsing logic
- [ ] Create default config template
- [ ] Add `tdo config init` command
- [ ] Load config on startup
- [ ] Apply colors from config
- [ ] Add `--no-color` flag
- [ ] Respect `NO_COLOR` env var
- [ ] Add `tdo config show` to display current config
- [ ] Add tests for config loading
- [ ] Update documentation

---

## Task 4: Better Progress Indicators

**Problem:** No visual feedback for long operations or batch processing

**Solution:** Add progress indicators for operations that may take time

**Expected Behavior:**
```bash
tdo done 1,2,3,4,5,6,7,8,9,10

Completing tasks...
[████████░░] 80% (8/10)

✓ Completed 10 tasks
  Failed: 0
  Skipped: 0
```

**Features:**
- Progress bars for bulk operations
- Spinners for single long operations
- Show operation count (X/Y)
- Show failures/errors during batch
- Don't show for single operations (instant)

**Implementation Details:**
- Add `indicatif` dependency for progress bars
- Detect batch operations vs single
- Show progress only if operation > 100ms
- Handle interrupts gracefully (Ctrl+C)

**Files to Change:**
- `crates/tdo/src/services/tasks.rs` - add progress callbacks
- `crates/tdo/src/main.rs` - render progress
- Add dependency: `indicatif`
- Tests: verify progress updates

**Estimated Effort:** 4-5 hours

**Steps:**
- [ ] Add `indicatif` dependency
- [ ] Detect bulk vs single operations
- [ ] Add progress bar for bulk operations
- [ ] Show operation count and percentage
- [ ] Handle operation failures gracefully
- [ ] Add spinner for async operations (future)
- [ ] Ensure progress bars don't break piping
- [ ] Test with various operation counts
- [ ] Update documentation

---

## Task 5: Improved Table/List Rendering

**Problem:** Task lists can be hard to scan, especially with many tasks

**Solution:** Add table borders, better spacing, visual grouping

**Expected Behavior:**
```bash
tdo view today --table

┌────┬───┬─────────────────────────────┬──────────────────────────────┐
│ ID │   │ Title                        │ Context                      │
├────┼───┼─────────────────────────────┼──────────────────────────────┤
│ 42 │ ● │ Review PR for auth           │ Work / api-project           │
│ 51 │ ○ │ Deploy to staging            │ Work / api-project           │
│ 67 │ ○ │ Write documentation          │ Work / api-project           │
└────┴───┴─────────────────────────────┴──────────────────────────────┘
```

**Features:**
- `--table` flag for bordered table view
- `--compact` flag for minimal spacing
- `--wide` flag to use full terminal width
- Alternating row colors (subtle)
- Column headers
- Better alignment for dates/metadata

**Implementation Details:**
- Use `comfy-table` or `tabled` library
- Add formatting options as flags
- Maintain current default view
- Make table rendering optional

**Files to Change:**
- `crates/tdo/src/ui.rs` - add table rendering mode
- `crates/tdo/src/main.rs` - add format flags
- Add dependency: `comfy-table` or `tabled`
- Tests: table rendering

**Estimated Effort:** 6-8 hours

**Steps:**
- [ ] Add table rendering library
- [ ] Create table rendering function
- [ ] Add `--table` flag to view commands
- [ ] Add `--compact` and `--wide` flags
- [ ] Implement column formatting
- [ ] Add alternating row colors (subtle)
- [ ] Handle long content (wrapping/truncation)
- [ ] Test with various terminal widths
- [ ] Make default format configurable
- [ ] Update documentation

---

## Task 6: Smart Task Numbering Display

**Problem:** Task numbers can get large (3-4 digits) and waste space

**Current:**
```
1234  ○  Task title
```

**Improved:**
```bash
# Show relative IDs in views (simpler)
1  ○  Task title                     (#1234)
2  ○  Another task                   (#1235)

# Or use smart padding
1234 ○  Task title
  42 ○  Another task
   7 ○  Third task
```

**Features:**
- Show short IDs in list views (1, 2, 3...)
- Show full ID in parentheses or on hover
- Smart padding for alignment
- `--show-full-ids` flag for traditional display

**Implementation Details:**
- Detect current view's task count
- Assign sequential numbers for display
- Keep actual task_number for operations
- Update UI rendering logic

**Files to Change:**
- `crates/tdo/src/ui.rs` - smart ID display
- Tests: ID rendering

**Estimated Effort:** 3-4 hours

**Steps:**
- [ ] Implement relative ID calculation
- [ ] Update task rendering to show relative IDs
- [ ] Show full ID in dimmed text
- [ ] Add `--show-full-ids` flag
- [ ] Ensure operations still use real task_number
- [ ] Test with large task numbers
- [ ] Update documentation

---

## Task 7: Context-Aware Help

**Problem:** Generic help text isn't specific to user's situation

**Current:**
```bash
tdo
No tasks for today

# User doesn't know what to do next
```

**Improved:**
```bash
tdo
No tasks for today 🎉

You have:
  • 3 tasks in inbox - use 'tdo view inbox' to see them
  • 5 upcoming tasks - use 'tdo view upcoming'

Get started:
  tdo add "Task title"           Add a new task
  tdo view inbox                 Process inbox
  tdo help                       See all commands
```

**Features:**
- Empty state messages with suggestions
- Context-aware tips based on user's data
- Show next logical action
- Onboarding tips for new users
- Hide tips with `TDO_QUIET=1` or after X uses

**Implementation Details:**
- Detect empty states
- Query store for relevant counts
- Show helpful next steps
- Track usage count for tip suppression

**Files to Change:**
- `crates/tdo/src/ui.rs` - add helper messages
- `crates/tdo/src/main.rs` - show tips in empty states
- Tests: verify tips appear correctly

**Estimated Effort:** 4-5 hours

**Steps:**
- [ ] Design empty state messages
- [ ] Add context detection logic
- [ ] Show relevant counts (inbox, upcoming)
- [ ] Add suggested next actions
- [ ] Add tip suppression logic
- [ ] Test with new vs established users
- [ ] Ensure tips don't clutter output
- [ ] Update documentation

---

## Task 8: Add JSON Output Mode

**Problem:** Output is human-readable but not machine-parseable

**Use Cases:**
- Shell scripting
- Integration with other tools
- Programmatic access to data

**Expected Behavior:**
```bash
tdo view today --json
{
  "view": "today",
  "date": "2026-02-19",
  "tasks": [
    {
      "id": "uuid...",
      "task_number": 42,
      "title": "Review PR",
      "status": "incomplete",
      "project": "api-project",
      "area": "Work",
      "tags": ["urgent"],
      "deadline": "2026-02-20T00:00:00Z"
    }
  ]
}
```

**Features:**
- `--json` flag on all view commands
- Structured JSON output
- Pretty-printed by default
- `--compact` for single-line JSON
- Include all task metadata

**Implementation Details:**
- Add `--json` flag to CLI
- Serialize tasks to JSON
- Suppress normal UI rendering
- Ensure stable schema (versioned)

**Files to Change:**
- `crates/tdo/src/main.rs` - add `--json` flag
- All view commands - check for JSON mode
- Tests: verify JSON output

**Estimated Effort:** 3-4 hours

**Steps:**
- [ ] Add `--json` flag to view commands
- [ ] Implement JSON serialization
- [ ] Suppress UI rendering in JSON mode
- [ ] Add `--compact` for single-line JSON
- [ ] Version the JSON schema
- [ ] Test JSON output for all views
- [ ] Update documentation with examples
- [ ] Add shell script examples

---

## Task 9: Enhanced Date Display

**Problem:** Date formatting is basic, not always clear

**Current:**
```
Feb 20
```

**Improved Options:**
```bash
# Relative (default)
tomorrow
in 2 days
next Monday

# ISO format
2026-02-20

# Short format
Feb 20

# Long format
Friday, Feb 20, 2026

# With relative hint
Feb 20 (in 2 days)
```

**Features:**
- Multiple date format options
- Configurable preference
- Smart relative dates ("tomorrow" vs "Feb 20")
- Combine absolute + relative: "Feb 20 (tomorrow)"
- Flag to override: `--date-format relative`

**Implementation Details:**
- Add date formatting functions for each style
- Read format from config
- Add CLI flag to override
- Keep display concise

**Files to Change:**
- `crates/tdo/src/ui.rs` - enhance date formatting
- `crates/tdo/src/config.rs` - add date format option
- Tests: date rendering

**Estimated Effort:** 4-5 hours

**Steps:**
- [ ] Add multiple date format functions
- [ ] Add config option for default format
- [ ] Add `--date-format` flag to override
- [ ] Implement relative date calculation
- [ ] Add combined format (absolute + relative)
- [ ] Test with various date ranges
- [ ] Ensure format fits in terminal
- [ ] Update documentation

---

## Task 10: Visual Task Status Indicators

**Problem:** Current glyphs (○ ●) are minimal, could be more expressive

**Current:**
```
○ = incomplete
● = overdue
✓ = completed
```

**Enhanced:**
```
▢ = todo
▣ = in progress (if tracking time)
✓ = done
✗ = cancelled
⊘ = blocked (dependencies not met)
⏸ = deferred
🔥 = urgent (high priority + deadline soon)
```

**Features:**
- More status types (in progress, blocked, deferred)
- Visual urgency indicators
- Customizable in config
- ASCII fallback for limited terminals

**Implementation Details:**
- Add status calculation logic
- Map status to appropriate glyph
- Make glyphs configurable
- Detect terminal capabilities (Unicode support)

**Files to Change:**
- `crates/tdo/src/ui.rs` - enhance glyph selection
- `crates/tdo/src/config.rs` - glyph customization
- Tests: glyph rendering

**Estimated Effort:** 3-4 hours

**Steps:**
- [ ] Design status-to-glyph mapping
- [ ] Add status detection logic
- [ ] Implement glyph rendering
- [ ] Add config for custom glyphs
- [ ] Add ASCII fallback mode
- [ ] Test with various terminal types
- [ ] Update design spec
- [ ] Update documentation

---

## Task 11: Keyboard Shortcuts Cheat Sheet

**Problem:** No quick reference for power users

**Solution:** Add `tdo shortcuts` or `tdo cheatsheet` command

**Expected Behavior:**
```bash
tdo shortcuts

TDO KEYBOARD SHORTCUTS

Views:
  tdo                        Today (default)
  tdo view inbox             Inbox
  tdo view upcoming          Upcoming tasks
  
Quick Add:
  tdo add "task"             Add to inbox
  tdo add "task" --today     Add to today
  
Actions:
  tdo done <id>              Complete task
  tdo done <id> <id> <id>    Complete multiple
  tdo move <id> --today      Reschedule
  
Tips:
  • Use task numbers (faster than names)
  • Use --json for scripting
  • Set up shell aliases for common commands
```

**Implementation Details:**
- Add new command: `Shortcuts` or `Cheatsheet`
- Organize by category
- Keep concise and scannable
- Maybe add `--print` to show as one-pager

**Files to Change:**
- `crates/tdo/src/main.rs` - add Shortcuts command
- Tests: verify output

**Estimated Effort:** 2-3 hours

**Steps:**
- [ ] Design cheat sheet layout
- [ ] Add `Shortcuts` command
- [ ] Organize by categories
- [ ] Keep concise (fit on one screen)
- [ ] Add `--full` for extended version
- [ ] Update documentation
- [ ] Maybe generate from help text?

---

## Priority Ranking (Suggested Order)

1. **JSON output mode** (4 hours) - Quick win, enables scripting
2. **Configuration file** (10 hours) - Foundation for customization
3. **Consistent entity rendering** (5 hours) - Polish existing views
4. **Context-aware help** (5 hours) - Better onboarding
5. **Progress indicators** (5 hours) - Better feedback
6. **Enhanced date display** (5 hours) - Improves readability
7. **Better table rendering** (8 hours) - Optional, advanced users
8. **Visual status indicators** (4 hours) - Nice polish
9. **Smart task numbering** (4 hours) - Minor improvement
10. **Keyboard shortcuts cheat** (3 hours) - Documentation
11. **Interactive TUI** (40 hours) - Major undertaking, optional

**Total Effort (excluding TUI):** ~53 hours (~1.5 weeks)  
**Total Effort (including TUI):** ~93 hours (~2.5 weeks)

---

## Quick Wins (Do First)

1. **JSON output mode** - Enables automation
2. **Context-aware help** - Better UX for new users
3. **Consistent rendering** - Fixes existing inconsistency

These three can be done in ~14 hours and immediately improve the experience.

---

## Long-term/Optional Features

- **Interactive TUI** - Major project, might be separate tool
- **Table rendering** - Nice to have, not essential
- **Smart task numbering** - Minor improvement, low priority

---

## Testing Requirements

Each UI improvement should include:
- [ ] Manual testing on different terminal sizes
- [ ] Testing with different terminal emulators
- [ ] Color/no-color testing
- [ ] Accessibility testing (screen readers if applicable)
- [ ] Documentation updates
- [ ] Screenshots/examples in docs
