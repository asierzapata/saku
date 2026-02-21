#![allow(deprecated)]

use assert_cmd::Command;
use assert_fs::TempDir;
use predicates::prelude::*;

fn tdo(temp_dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("tdo").unwrap();
    cmd.env("TDO_DATA_DIR", temp_dir.path());
    cmd
}

// ── Empty states ──────────────────────────────────────────────────────────────

#[test]
fn view_today_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["view", "today"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tasks for today"));
}

#[test]
fn view_inbox_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["view", "inbox"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Inbox is empty"));
}

#[test]
fn view_upcoming_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["view", "upcoming"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No upcoming tasks"));
}

#[test]
fn view_someday_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["view", "someday"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No someday tasks"));
}

#[test]
fn view_logbook_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["view", "logbook"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "No completed tasks in the last 14 days",
        ));
}

#[test]
fn view_trash_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["view", "trash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Trash is empty"));
}

#[test]
fn view_all_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["view", "all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No active tasks"));
}

// ── With data ─────────────────────────────────────────────────────────────────

#[test]
fn view_today_shows_today_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Morning standup", "--today"])
        .assert()
        .success();

    tdo(&temp)
        .args(["view", "today"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Morning standup"))
        .stdout(predicate::str::contains("Today"));
}

#[test]
fn view_inbox_shows_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp).args(["add", "Call dentist"]).assert().success();

    tdo(&temp)
        .args(["view", "inbox"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Call dentist"))
        .stdout(predicate::str::contains("Inbox"));
}

#[test]
fn view_upcoming_shows_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Team meeting", "--on", "2030-06-15"])
        .assert()
        .success();

    tdo(&temp)
        .args(["view", "upcoming"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Team meeting"));
}

#[test]
fn view_someday_shows_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Learn Rust", "--someday"])
        .assert()
        .success();

    tdo(&temp)
        .args(["view", "someday"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Learn Rust"));
}

#[test]
fn view_logbook_shows_completed_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp).args(["add", "Buy milk"]).assert().success();
    tdo(&temp).args(["done", "1"]).assert().success();

    tdo(&temp)
        .args(["view", "logbook"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Buy milk"));
}

#[test]
fn view_trash_shows_deleted_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp).args(["add", "Buy milk"]).assert().success();
    tdo(&temp).args(["delete", "1"]).assert().success();

    tdo(&temp)
        .args(["view", "trash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Buy milk"));
}

#[test]
fn view_all_shows_tasks_grouped() {
    let temp = TempDir::new().unwrap();

    tdo(&temp).args(["add", "Inbox task"]).assert().success();
    tdo(&temp)
        .args(["add", "Today task", "--today"])
        .assert()
        .success();
    tdo(&temp)
        .args(["add", "Someday task", "--someday"])
        .assert()
        .success();

    tdo(&temp)
        .args(["view", "all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Inbox task"))
        .stdout(predicate::str::contains("Today task"))
        .stdout(predicate::str::contains("Someday task"));
}

// ── Entity views ──────────────────────────────────────────────────────────────

#[test]
fn view_project_shows_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "project", "Website"])
        .assert()
        .success();

    tdo(&temp)
        .args(["add", "Fix bug", "--project", "Website"])
        .assert()
        .success();

    tdo(&temp)
        .args(["view", "project", "Website"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fix bug"));
}

#[test]
fn view_area_shows_projects_and_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .success();

    tdo(&temp)
        .args(["create", "project", "Website", "--area", "Work"])
        .assert()
        .success();

    tdo(&temp)
        .args(["add", "Fix bug", "--project", "Website"])
        .assert()
        .success();

    tdo(&temp)
        .args(["view", "area", "Work"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fix bug"));
}

#[test]
fn view_tag_shows_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Fix bug", "--tag", "urgent"])
        .assert()
        .success();

    tdo(&temp)
        .args(["view", "tag", "urgent"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fix bug"));
}

#[test]
fn view_tag_not_found() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["view", "tag", "nonexistent"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tasks with tag 'nonexistent'"));
}

#[test]
fn view_project_not_found() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["view", "project", "Nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Error: Project 'Nonexistent' not found",
        ));
}

#[test]
fn view_area_not_found() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["view", "area", "Nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Error: Area 'Nonexistent' not found",
        ));
}

// ── Ordering ───────────────────────────────────────────────────────────────────

fn assert_order(stdout: &str, first: &str, second: &str) {
    let pos_first = stdout.find(first).unwrap_or_else(|| {
        panic!("'{first}' not found in output:\n{stdout}");
    });
    let pos_second = stdout.find(second).unwrap_or_else(|| {
        panic!("'{second}' not found in output:\n{stdout}");
    });
    assert!(
        pos_first < pos_second,
        "Expected '{first}' (pos {pos_first}) to appear before '{second}' (pos {pos_second})"
    );
}

#[test]
fn view_inbox_tasks_appear_in_number_order() {
    let temp = TempDir::new().unwrap();

    // Add tasks — they get numbers 1, 2, 3 in creation order
    tdo(&temp).args(["add", "ALPHA_TASK"]).assert().success();
    tdo(&temp).args(["add", "BETA_TASK"]).assert().success();
    tdo(&temp).args(["add", "GAMMA_TASK"]).assert().success();

    let assert = tdo(&temp).args(["view", "inbox"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert_order(&stdout, "ALPHA_TASK", "BETA_TASK");
    assert_order(&stdout, "BETA_TASK", "GAMMA_TASK");
}

// ── Deadlines view ────────────────────────────────────────────────────────────

#[test]
fn view_deadlines_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["view", "deadlines"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tasks with deadlines"));
}

#[test]
fn view_deadlines_shows_task_with_deadline() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Ship the feature", "--due", "2030-06-15"])
        .assert()
        .success();

    tdo(&temp)
        .args(["view", "deadlines"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Ship the feature"))
        .stdout(predicate::str::contains("Deadlines"));
}

#[test]
fn view_deadlines_does_not_show_tasks_without_deadline() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "No deadline task"])
        .assert()
        .success();

    tdo(&temp)
        .args(["view", "deadlines"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tasks with deadlines"));
}

#[test]
fn view_deadlines_groups_overdue_before_later() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "OVERDUE_TASK", "--due", "2020-01-01"])
        .assert()
        .success();

    tdo(&temp)
        .args(["add", "FUTURE_TASK", "--due", "2030-12-31"])
        .assert()
        .success();

    let assert = tdo(&temp).args(["view", "deadlines"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert_order(&stdout, "Overdue", "OVERDUE_TASK");
    assert_order(&stdout, "OVERDUE_TASK", "Later");
    assert_order(&stdout, "Later", "FUTURE_TASK");
}

#[test]
fn view_deadlines_sorts_by_deadline_date() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "LATER_DEADLINE", "--due", "2030-12-31"])
        .assert()
        .success();

    tdo(&temp)
        .args(["add", "EARLIER_DEADLINE", "--due", "2030-06-01"])
        .assert()
        .success();

    let assert = tdo(&temp).args(["view", "deadlines"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert_order(&stdout, "EARLIER_DEADLINE", "LATER_DEADLINE");
}

#[test]
fn view_deadlines_does_not_show_completed_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Done task", "--due", "2030-06-15"])
        .assert()
        .success();

    tdo(&temp).args(["done", "1"]).assert().success();

    tdo(&temp)
        .args(["view", "deadlines"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tasks with deadlines"));
}

#[test]
fn view_today_tasks_appear_in_number_order() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "ALPHA_TASK", "--today"])
        .assert()
        .success();
    tdo(&temp)
        .args(["add", "BETA_TASK", "--today"])
        .assert()
        .success();
    tdo(&temp)
        .args(["add", "GAMMA_TASK", "--today"])
        .assert()
        .success();

    let assert = tdo(&temp).args(["view", "today"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert_order(&stdout, "ALPHA_TASK", "BETA_TASK");
    assert_order(&stdout, "BETA_TASK", "GAMMA_TASK");
}
