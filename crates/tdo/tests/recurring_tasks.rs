#![allow(deprecated)]

use assert_cmd::Command;
use assert_fs::TempDir;
use predicates::prelude::*;

fn tdo(temp_dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("tdo").unwrap();
    cmd.env("TDO_DATA_DIR", temp_dir.path());
    cmd
}

// ─── Add with recurrence ────────────────────────────────────────────────────

#[test]
fn add_daily_recurring_task() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Take vitamins", "--every", "daily"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task added: Take vitamins"))
        .stdout(predicate::str::contains("↻ Repeats:"));
}

#[test]
fn add_weekly_recurring_task() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Team sync", "--every", "weekly"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task added: Team sync"))
        .stdout(predicate::str::contains("↻ Repeats:"));
}

#[test]
fn add_recurring_task_on_specific_weekdays() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Gym session", "--every", "mon,wed,fri"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task added: Gym session"))
        .stdout(predicate::str::contains("↻ Repeats:"));
}

#[test]
fn add_monthly_recurring_task() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Pay rent", "--every", "monthly"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task added: Pay rent"))
        .stdout(predicate::str::contains("↻ Repeats:"));
}

#[test]
fn add_yearly_recurring_task() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "File taxes", "--every", "yearly"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task added: File taxes"))
        .stdout(predicate::str::contains("↻ Repeats:"));
}

#[test]
fn add_recurring_task_with_until_date() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Sprint standup", "--every", "daily", "--until", "2030-12-31"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task added: Sprint standup"))
        .stdout(predicate::str::contains("↻ Repeats:"));
}

#[test]
fn add_recurring_task_invalid_pattern_fails() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Mystery task", "--every", "biweekly"])
        .assert()
        .failure();
}

// ─── View recurring list ────────────────────────────────────────────────────

#[test]
fn view_recurring_shows_recurring_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Take vitamins", "--every", "daily"])
        .assert()
        .success();

    tdo(&temp)
        .args(["add", "One-off task"])
        .assert()
        .success();

    tdo(&temp)
        .args(["view", "recurring"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Take vitamins"))
        .stdout(predicate::str::is_match("↻").unwrap());
}

#[test]
fn view_recurring_does_not_show_non_recurring_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Regular task"])
        .assert()
        .success();

    tdo(&temp)
        .args(["view", "recurring"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Regular task").not());
}

// ─── Complete occurrence vs. stop ───────────────────────────────────────────

#[test]
fn done_on_recurring_task_marks_occurrence_not_completes() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Daily review", "--every", "daily"])
        .assert()
        .success();

    // Completing without --stop should mark an occurrence, not permanently complete
    tdo(&temp)
        .args(["done", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("↻ Occurrence marked done:").or(
            predicate::str::contains("Done:")
        ));

    // Task should still appear in the recurring list (not permanently completed)
    tdo(&temp)
        .args(["view", "recurring"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Daily review"));
}

#[test]
fn done_with_stop_permanently_completes_recurring_task() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Daily review", "--every", "daily"])
        .assert()
        .success();

    // Completing with --stop should permanently complete the task
    tdo(&temp)
        .args(["done", "1", "--stop"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task completed:"));

    // Task should no longer appear in recurring list
    tdo(&temp)
        .args(["view", "recurring"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Daily review").not());
}

// ─── Move: set/update/clear recurrence ──────────────────────────────────────

#[test]
fn move_can_add_recurrence_to_existing_task() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Water plants"])
        .assert()
        .success();

    tdo(&temp)
        .args(["move", "1", "--every", "weekly"])
        .assert()
        .success();

    // Now it should appear in recurring list
    tdo(&temp)
        .args(["view", "recurring"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Water plants"));
}

#[test]
fn move_can_clear_recurrence_from_task() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Recurring task", "--every", "daily"])
        .assert()
        .success();

    // Clear the recurrence
    tdo(&temp)
        .args(["move", "1", "--clear-recurrence"])
        .assert()
        .success();

    // Should no longer appear in recurring list
    tdo(&temp)
        .args(["view", "recurring"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Recurring task").not());
}

// ─── Recurrence badge in views ───────────────────────────────────────────────

#[test]
fn recurring_task_shows_badge_in_inbox() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Daily standup", "--every", "daily"])
        .assert()
        .success();

    tdo(&temp)
        .args(["inbox"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Daily standup"))
        .stdout(predicate::str::is_match("↻").unwrap());
}
