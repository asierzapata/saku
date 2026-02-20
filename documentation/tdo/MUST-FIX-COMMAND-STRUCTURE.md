# Must Fix - Command Structure Consistency

**Priority:** P0 (Critical)  
**Estimated Total Effort:** 1 week  
**Rationale:** Inconsistent command structure confuses users and makes the CLI harder to learn

---

## Problem Statement

The TDO CLI currently has mixed command patterns:
- **Direct view commands:** `tdo today`, `tdo inbox`, `tdo someday`
- **Nested entity commands:** `tdo create area`, `tdo list areas`, `tdo show project`

This inconsistency creates a poor user experience where users must remember which commands are nested and which aren't.

---

## Solution: Nest All View Commands Under `tdo view`

Standardize on nested subcommands for all view operations to create a consistent, predictable CLI structure.

---

## Task 1: Create `tdo view` Subcommand Structure

**Objective:** Reorganize all view commands under a single `view` subcommand

**Before:**
```bash
tdo today
tdo inbox
tdo upcoming
tdo someday
tdo logbook
tdo trash
tdo all
```

**After:**
```bash
tdo view today
tdo view inbox
tdo view upcoming
tdo view someday
tdo view logbook
tdo view trash
tdo view all
```

**Implementation Steps:**
- [ ] Add `View` subcommand enum in `main.rs`
- [ ] Create nested subcommand structure
- [ ] Move all view logic under `Commands::View { subcommand }`
- [ ] Keep default behavior: `tdo` (no args) shows today

**Estimated Effort:** 2-3 hours

**Code Location:** `crates/tdo/src/main.rs`

---

## Task 2: Add `tdo view` Subcommands for Entity Views

**Objective:** Move entity-specific views under consistent structure

**Before:**
```bash
tdo show project "name"
tdo show area "name"
tdo show tag "name"
```

**After:**
```bash
tdo view project "name"
tdo view area "name"
tdo view tag "name"
```

**Rationale:** "show" is redundant - all these commands show/view data

**Implementation Steps:**
- [ ] Add `Project`, `Area`, `Tag` variants to `View` subcommand
- [ ] Move logic from `Commands::Show` to `Commands::View`
- [ ] Update help text to clarify usage

**Estimated Effort:** 1-2 hours

---

## Task 3: Standardize List Commands Under `tdo list`

**Objective:** Keep all listing operations under `list` for consistency

**Current (Good):**
```bash
tdo list areas
tdo list projects
tdo list tags
```

**Keep as-is:** This structure is already consistent and intuitive

**Implementation Steps:**
- [ ] No code changes needed
- [ ] Update documentation to clarify list vs view distinction

**Estimated Effort:** 30 minutes (docs only)

---

## Task 4: Deprecate Old Direct View Commands (Backward Compatibility)

**Objective:** Support old commands temporarily while users migrate

**Strategy:**
- Keep `tdo today`, `tdo inbox`, etc. working
- Print deprecation warning directing users to `tdo view today`
- Remove completely in v1.0.0

**Implementation Steps:**
- [ ] Add deprecation warnings to old commands
- [ ] Log usage of deprecated commands (if logging enabled)
- [ ] Update help text to show new syntax
- [ ] Set timeline for removal (e.g., "deprecated in v0.5, removed in v1.0")

**Example deprecation message:**
```
Warning: 'tdo today' is deprecated. Use 'tdo view today' instead.
This command will be removed in v1.0.0.
```

**Estimated Effort:** 2-3 hours

---

## Task 5: Update All Documentation

**Objective:** Ensure all docs reflect new command structure

**Files to update:**
- [ ] `README.md` - update all command examples
- [ ] `documentation/tdo/commands-cheat-sheet.md` - rewrite view section
- [ ] `documentation/tdo/design-spec.md` - update mockup examples
- [ ] `crates/tdo/README.md` - update usage section
- [ ] `skills/saku-integration/SKILL.md` - update command list

**Key changes:**
- Replace all `tdo today` → `tdo view today`
- Add new section explaining command structure
- Update all code examples
- Add migration guide for existing users

**Estimated Effort:** 2-3 hours

---

## Task 6: Update Shell Completions

**Objective:** Ensure tab completion works with new structure

**Implementation Steps:**
- [ ] Regenerate completions with new command structure
- [ ] Test completion for: `tdo view <TAB>`
- [ ] Test completion for entity names: `tdo view project <TAB>`
- [ ] Update documentation on generating completions

**Command:**
```bash
tdo completion bash > /etc/bash_completion.d/tdo
tdo completion zsh > /usr/share/zsh/site-functions/_tdo
tdo completion fish > ~/.config/fish/completions/tdo.fish
```

**Estimated Effort:** 1 hour

---

## Task 7: Update Integration Tests

**Objective:** Change all tests to use new command structure

**Files to update:**
- [ ] `tests/task_commands.rs` - keep as-is (non-view commands)
- [ ] `tests/view_commands.rs` - update to use `tdo view ...`
- [ ] `tests/show_commands.rs` - merge into view_commands.rs
- [ ] `tests/area_commands.rs` - keep as-is (non-view commands)
- [ ] `tests/project_commands.rs` - keep as-is (non-view commands)

**Estimated Effort:** 2-3 hours

---

## Proposed New Command Structure

### Viewing Data
```bash
tdo view today              # Today's tasks
tdo view inbox              # Unscheduled tasks
tdo view upcoming           # Future tasks
tdo view someday            # Someday/maybe
tdo view logbook            # Completed tasks
tdo view trash              # Deleted items
tdo view all                # All active tasks

tdo view project "name"     # Tasks in project
tdo view area "name"        # Tasks/projects in area
tdo view tag "name"         # Tasks with tag
```

### Listing Entities
```bash
tdo list areas              # All areas
tdo list projects           # All projects
tdo list tags               # All tags
```

### Creating/Modifying
```bash
tdo add "task"              # Add task
tdo move <id> --today       # Move task
tdo done <id>               # Complete task
tdo delete <id>             # Delete task
tdo restore <id>            # Restore task
tdo edit task <id>          # Edit task

tdo create area "name"      # Create area
tdo create project "name"   # Create project

tdo edit area "name"        # Edit area
tdo edit project "name"     # Edit project

tdo remove area "name"      # Delete area
tdo remove project "name"   # Delete project
```

### Special Cases
```bash
tdo                         # Default: same as 'tdo view today'
tdo completion <shell>      # Generate completions
```

---

## Benefits of This Structure

### ✅ Consistency
- All viewing operations under `view`
- All listing operations under `list`
- All creation under `create`
- All editing under `edit`
- All deletion under `remove`

### ✅ Discoverability
- Users can type `tdo view <TAB>` to see all options
- Command structure is predictable
- Help text is better organized

### ✅ Extensibility
- Easy to add new view types: `tdo view deadlines`, `tdo view overdue`
- Clear where new commands should go
- Namespace prevents conflicts

### ✅ Learning Curve
- New users learn one pattern, apply everywhere
- No exceptions or special cases to remember
- Help text groups related commands together

---

## Migration Guide for Users

**For users upgrading from v0.4 to v0.5:**

Old commands will continue to work but show deprecation warnings:

```bash
# Old (still works in v0.5)
tdo today
⚠ Warning: 'tdo today' is deprecated. Use 'tdo view today' instead.

# New (recommended)
tdo view today
```

**Update your scripts/aliases:**
```bash
# In your ~/.bashrc or ~/.zshrc
alias tdt='tdo view today'
alias tdi='tdo view inbox'
alias tdu='tdo view upcoming'
```

**Old commands will be removed in v1.0.0** (at least 3 months from v0.5 release)

---

## Rollout Plan

### Phase 1: Implementation (Week 1)
- Day 1-2: Implement new command structure
- Day 3: Add deprecation warnings
- Day 4: Update tests
- Day 5: Update documentation

### Phase 2: Release v0.5 (Week 2)
- Announce deprecation in release notes
- Update website/docs
- Post migration guide

### Phase 3: Deprecation Period (3-6 months)
- Monitor usage of old commands
- Remind users in monthly announcements
- Give time for scripts/workflows to update

### Phase 4: Release v1.0 (3-6 months later)
- Remove old direct view commands
- Clean command structure
- Major version bump (breaking change)

---

## Alternative Considered (Not Recommended)

**Flatten everything to top level:**
```bash
tdo today
tdo inbox
tdo areas
tdo projects
tdo tags
```

**Why rejected:**
- Namespace pollution (dozens of top-level commands)
- Harder to discover (need to know exact command name)
- No logical grouping
- Doesn't scale as features are added

---

## Success Criteria

- [ ] All view operations use `tdo view <subcommand>` pattern
- [ ] Old commands still work with deprecation warnings
- [ ] All tests pass
- [ ] All documentation updated
- [ ] Shell completions work correctly
- [ ] Zero regressions in functionality
- [ ] Users can easily migrate (clear guide + warnings)

---

**Total Estimated Effort:** 1 week (40 hours)

**Breakdown:**
- Code changes: 8-10 hours
- Testing: 5-7 hours
- Documentation: 4-5 hours
- Shell completions: 1 hour
- Review and polish: 2-3 hours

**Priority:** This should be done before adding new features to avoid needing to refactor twice.
