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
fn default_view_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .assert()
        .success()
        .stdout(predicate::str::contains("No tasks for today"));
}

#[test]
fn today_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["today"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tasks for today"));
}

#[test]
fn inbox_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["inbox"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Inbox is empty"));
}

#[test]
fn upcoming_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["upcoming"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No upcoming tasks"));
}

#[test]
fn anytime_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["anytime"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No anytime tasks"));
}

#[test]
fn someday_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["someday"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No someday tasks"));
}

#[test]
fn logbook_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["logbook"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "No completed tasks in the last 14 days",
        ));
}

#[test]
fn trash_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["trash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Trash is empty"));
}

#[test]
fn all_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No active tasks"));
}

#[test]
fn tag_list_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["tag", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tags found"));
}

// ── With data ─────────────────────────────────────────────────────────────────

#[test]
fn today_shows_today_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Morning standup", "--today"])
        .assert()
        .success();

    tdo(&temp)
        .args(["today"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Morning standup"))
        .stdout(predicate::str::contains("Today"));
}

#[test]
fn inbox_shows_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Call dentist"])
        .assert()
        .success();

    tdo(&temp)
        .args(["inbox"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Call dentist"))
        .stdout(predicate::str::contains("Inbox"));
}

#[test]
fn upcoming_shows_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Team meeting", "--when", "2030-06-15"])
        .assert()
        .success();

    tdo(&temp)
        .args(["upcoming"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Team meeting"));
}

#[test]
fn anytime_shows_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Read a book", "--anytime"])
        .assert()
        .success();

    tdo(&temp)
        .args(["anytime"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Read a book"));
}

#[test]
fn someday_shows_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Learn Rust", "--someday"])
        .assert()
        .success();

    tdo(&temp)
        .args(["someday"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Learn Rust"));
}

#[test]
fn logbook_shows_completed_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp).args(["add", "Buy milk"]).assert().success();
    tdo(&temp).args(["done", "1"]).assert().success();

    tdo(&temp)
        .args(["logbook"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Buy milk"));
}

#[test]
fn trash_shows_deleted_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp).args(["add", "Buy milk"]).assert().success();
    tdo(&temp).args(["delete", "1"]).assert().success();

    tdo(&temp)
        .args(["trash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Buy milk"));
}

#[test]
fn all_shows_tasks_grouped() {
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
        .args(["all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Inbox task"))
        .stdout(predicate::str::contains("Today task"))
        .stdout(predicate::str::contains("Someday task"));
}

#[test]
fn tag_list_shows_tags() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Fix bug", "--tag", "urgent"])
        .assert()
        .success();

    tdo(&temp)
        .args(["tag", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("urgent"));
}

#[test]
fn tag_view_shows_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Fix bug", "--tag", "urgent"])
        .assert()
        .success();

    tdo(&temp)
        .args(["tag", "view", "urgent"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fix bug"));
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
fn inbox_tasks_appear_in_number_order() {
    let temp = TempDir::new().unwrap();

    // Add tasks — they get numbers 1, 2, 3 in creation order
    tdo(&temp).args(["add", "ALPHA_TASK"]).assert().success();
    tdo(&temp).args(["add", "BETA_TASK"]).assert().success();
    tdo(&temp).args(["add", "GAMMA_TASK"]).assert().success();

    let assert = tdo(&temp).args(["inbox"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert_order(&stdout, "ALPHA_TASK", "BETA_TASK");
    assert_order(&stdout, "BETA_TASK", "GAMMA_TASK");
}

#[test]
fn inbox_tasks_with_deadlines_appear_first() {
    let temp = TempDir::new().unwrap();

    tdo(&temp).args(["add", "NO_DEADLINE"]).assert().success();
    tdo(&temp)
        .args(["add", "WITH_DEADLINE", "--deadline", "2030-01-01"])
        .assert()
        .success();

    let assert = tdo(&temp).args(["inbox"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert_order(&stdout, "WITH_DEADLINE", "NO_DEADLINE");
}

#[test]
fn today_tasks_appear_in_number_order() {
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

    let assert = tdo(&temp).args(["today"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    assert_order(&stdout, "ALPHA_TASK", "BETA_TASK");
    assert_order(&stdout, "BETA_TASK", "GAMMA_TASK");
}
