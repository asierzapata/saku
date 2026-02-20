# Code Quality Issues - TDO CLI

**Priority:** P2 (Technical Debt)  
**Estimated Total Effort:** 1-2 weeks  

---

## 🐛 Dead Code Warnings

### Task 1: Remove or Use `format_completion_date` Function

**File:** `crates/tdo/src/ui.rs:276`

**Issue:**
```
warning: function `format_completion_date` is never used
   --> crates/tdo/src/ui.rs:276:4
```

**Context:**
- Function exists but is not called anywhere in the codebase
- Likely intended for logbook view improvements
- Could be used to show human-readable completion dates instead of raw timestamps

**Options:**
1. **Use it:** Integrate into logbook view to show "Completed 2 days ago" instead of timestamps
2. **Remove it:** Delete the function if not needed

**Recommendation:** Use it in logbook view for better UX

**Estimated Effort:** 1 hour

**Steps:**
- [ ] Review function implementation
- [ ] Decide if it should be used in logbook view
- [ ] If yes: integrate into `render_task_line_with_completion_date()`
- [ ] If no: remove function and `#[allow(dead_code)]` attribute
- [ ] Test logbook view output

---

## 📎 Clippy Warnings

### Task 2: Fix `io_other_error` Warnings (5 occurrences)

**Files:** Multiple locations in storage/editor code

**Issue:**
```
warning: this can be `std::io::Error::other(_)`
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.92.0/index.html#io_other_error
```

**Context:**
- Old-style error construction: `io::Error::new(io::ErrorKind::Other, msg)`
- New Rust idiom: `io::Error::other(msg)` (cleaner, more concise)

**Example:**
```rust
// Old (current)
io::Error::new(io::ErrorKind::Other, "failed to read")

// New (recommended)
io::Error::other("failed to read")
```

**Estimated Effort:** 30 minutes

**Steps:**
- [ ] Run `cargo clippy --fix --allow-dirty`
- [ ] Review all automatic fixes
- [ ] Manually fix any remaining instances
- [ ] Test that errors still work correctly

---

### Task 3: Collapse Nested If Statements

**Files:** Unknown (need to run clippy to identify)

**Issue:**
```
warning: this `else { if .. }` block can be collapsed
```

**Context:**
- Code has `else { if ... }` patterns that should be `else if`
- Reduces nesting, improves readability

**Example:**
```rust
// Current (bad)
if condition1 {
    // ...
} else {
    if condition2 {
        // ...
    }
}

// Fixed (good)
if condition1 {
    // ...
} else if condition2 {
    // ...
}
```

**Estimated Effort:** 15 minutes

**Steps:**
- [ ] Run `cargo clippy` to identify locations
- [ ] Run `cargo clippy --fix` to auto-fix
- [ ] Verify code still compiles
- [ ] Review changes for correctness

---

### Task 4: Fix Manual `RangeInclusive::contains` Implementation

**Files:** Unknown (need to run clippy to identify)

**Issue:**
```
warning: manual `RangeInclusive::contains` implementation
```

**Context:**
- Code manually checks if value is within range: `x >= start && x <= end`
- Rust has built-in method: `(start..=end).contains(&x)`

**Example:**
```rust
// Current (manual)
if urgency_days >= 1 && urgency_days <= 3 {
    // ...
}

// Fixed (idiomatic)
if (1..=3).contains(&urgency_days) {
    // ...
}
```

**Estimated Effort:** 15 minutes

**Steps:**
- [ ] Run `cargo clippy` to find exact location
- [ ] Replace manual range checks with `.contains()`
- [ ] Test that logic still works correctly

---

### Task 5: Fix Warnings in Dependencies

**Files:** `saku-storage` and `saku-sync` crates

**Issue:**
```
warning: this `impl` can be derived
warning: `saku-storage` (lib) generated 1 warning

warning: this `if` statement can be collapsed  
warning: `saku-sync` (lib) generated 1 warning
```

**Context:**
- Workspace dependencies have clippy warnings
- Should be fixed for clean builds

**Estimated Effort:** 30 minutes

**Steps:**
- [ ] `cd crates/saku-storage && cargo clippy --fix`
- [ ] `cd crates/saku-sync && cargo clippy --fix`
- [ ] Review and test changes
- [ ] Commit fixes separately for each crate

---

## 🧪 Test Coverage Gaps

### Task 6: Add Negative Test Cases for Error Paths

**Context:**
- Current tests focus on happy paths
- Need tests for error conditions: not found, ambiguous names, invalid input

**Missing test coverage:**
- Task not found errors
- Ambiguous name resolution (multiple matches)
- Invalid date formats
- Conflicting command flags
- Project/area not found errors
- Already deleted/completed errors

**Estimated Effort:** 3-4 hours

**Steps:**
- [ ] Review existing test files (`tests/task_commands.rs`, etc.)
- [ ] Add test cases for each error condition
- [ ] Test ambiguous name matching (2+ matches)
- [ ] Test invalid date inputs
- [ ] Test edge cases (empty title, very long strings)
- [ ] Verify error messages are user-friendly

**Example tests to add:**
```rust
#[test]
fn test_done_task_not_found() {
    // Verify error message when task doesn't exist
}

#[test]
fn test_done_ambiguous_name() {
    // Add 2 tasks with similar names, verify error
}

#[test]
fn test_add_with_invalid_date() {
    // Test various malformed date strings
}
```

---

### Task 7: Add Concurrent Access Tests

**Context:**
- Storage uses file locking but no tests verify it works
- Race conditions could cause data corruption
- Need tests that simulate concurrent operations

**Estimated Effort:** 2-3 hours

**Steps:**
- [ ] Create test that spawns multiple threads
- [ ] Each thread tries to add/modify tasks simultaneously
- [ ] Verify file locking prevents corruption
- [ ] Verify all operations complete successfully
- [ ] Test lock timeout behavior

**Example test:**
```rust
#[test]
fn test_concurrent_task_creation() {
    // Spawn 10 threads, each adds a task
    // Verify all 10 tasks exist after completion
    // Verify no data corruption
}
```

---

### Task 8: Add Migration Tests with Real Data Files

**Context:**
- Migrations exist (v1→v2→v3→v4) but aren't tested with real legacy data
- Risk of migration bugs breaking user data
- Need test fixtures with actual old format JSON

**Estimated Effort:** 3-4 hours

**Steps:**
- [ ] Create test fixtures: `test_data/store_v1.json`, `v2.json`, `v3.json`
- [ ] Add test that loads v1 data and verifies migration to v4
- [ ] Test each migration step individually (v1→v2, v2→v3, v3→v4)
- [ ] Verify data integrity after migration
- [ ] Test edge cases (empty store, large store, malformed data)

**Test files to create:**
```
crates/tdo/tests/fixtures/
├── store_v1.json          # Old format with no version field
├── store_v2.json          # Has task_numbers but no deleted_at
├── store_v3.json          # Has soft deletes
└── store_v4.json          # Current format
```

---

### Task 9: Add Integration Tests for Complex Workflows

**Context:**
- Existing integration tests cover individual commands
- Need tests for realistic multi-step workflows

**Missing workflow tests:**
- Create project → add tasks → complete some → view different lists
- Create area → create projects → move tasks between projects
- Delete project → verify cascade to tasks
- Restore from trash → verify task is active again
- Edit task → change multiple fields → verify all changes

**Estimated Effort:** 2-3 hours

**Steps:**
- [ ] Create `tests/workflows.rs`
- [ ] Add test for complete project lifecycle
- [ ] Add test for area/project hierarchy
- [ ] Add test for cascade deletes
- [ ] Add test for trash/restore workflow
- [ ] Verify final state matches expectations

---

### Task 10: Add Property-Based Tests for Date Parsing

**Context:**
- Date parsing is complex with many edge cases
- Current tests use fixed examples
- Property-based testing could find edge cases

**Estimated Effort:** 2-3 hours

**Steps:**
- [ ] Add `proptest` or `quickcheck` dependency
- [ ] Create property test for date parsing
- [ ] Test that parsed dates are always valid
- [ ] Test that relative dates ("monday") are always in future
- [ ] Test round-trip: parse → format → parse = same date

**Example:**
```rust
#[test]
fn test_weekday_parsing_is_always_future() {
    // For any weekday, parsed date should be >= tomorrow
}
```

---

## 📊 Test Coverage Report (Current State)

**Overall Coverage:** Estimated ~50-60%

**Well-tested:**
- ✅ Service layer (tasks, projects, areas) - ~80% coverage
- ✅ Model layer (task ordering, date parsing) - ~70% coverage
- ✅ Storage layer (load/save operations) - ~60% coverage

**Under-tested:**
- ⚠️ Error paths - ~20% coverage
- ⚠️ Edge cases - minimal coverage
- ⚠️ Concurrent access - no tests
- ⚠️ Migrations - basic tests only
- ⚠️ UI rendering - no tests

---

## 🎯 Quick Wins (Do These First)

1. **Fix all clippy warnings** (1 hour) - run `cargo clippy --fix`
2. **Remove dead code** (1 hour) - decide on `format_completion_date`
3. **Add basic negative tests** (2 hours) - task not found, ambiguous names

These three tasks will immediately improve code quality with minimal effort.

---

## 📝 Testing Best Practices to Adopt

### Recommendations for future development:

1. **Write tests first (TDD)** - especially for bug fixes
2. **Test error paths** - every `Err` variant should have a test
3. **Use descriptive test names** - `test_done_task_not_found` not `test_error_1`
4. **Test edge cases** - empty strings, max values, special characters
5. **Add integration tests** - for user-facing workflows
6. **Run clippy in CI** - prevent new warnings from merging

---

## 🔧 Tooling Setup

To catch these issues earlier:

```bash
# Run before committing
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace

# Add to .git/hooks/pre-commit
#!/bin/bash
cargo clippy --all-targets --all-features -- -D warnings || exit 1
cargo test --workspace || exit 1
```

---

**Total Estimated Effort:** 15-20 hours (spread across 1-2 weeks)

**Priority Order:**
1. Clippy warnings (quick wins)
2. Dead code removal
3. Negative test cases
4. Migration tests
5. Concurrent access tests
6. Property-based tests
