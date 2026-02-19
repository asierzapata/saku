use assert_cmd::Command;
use assert_fs::TempDir;
use predicates::prelude::*;

fn tdo(temp_dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("tdo").unwrap();
    cmd.env("TDO_DATA_DIR", temp_dir.path());
    cmd
}

#[test]
fn add_task_default_goes_to_inbox() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Buy milk"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task added: Buy milk"))
        .stdout(predicate::str::contains("#1"));

    tdo(&temp)
        .args(["inbox"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Buy milk"));
}

#[test]
fn add_task_today() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Morning standup", "--today"])
        .assert()
        .success();

    tdo(&temp)
        .args(["today"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Morning standup"));
}

#[test]
fn add_task_someday() {
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
fn add_task_with_on_date() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Team meeting", "--on", "2030-06-15"])
        .assert()
        .success();

    tdo(&temp)
        .args(["upcoming"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Team meeting"));
}

#[test]
fn add_task_with_project() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "project", "Website"])
        .assert()
        .success();

    tdo(&temp)
        .args(["add", "Fix bug", "--project", "website"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Project: Website"));
}

#[test]
fn add_task_with_area() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .success();

    tdo(&temp)
        .args(["add", "Report", "--area", "work"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task added: Report"));
}

#[test]
fn add_task_with_tags() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["add", "Fix bug", "--tag", "urgent", "--tag", "backend"])
        .assert()
        .success();

    tdo(&temp)
        .args(["list", "tags"])
        .assert()
        .success()
        .stdout(predicate::str::contains("urgent"))
        .stdout(predicate::str::contains("backend"));
}

#[test]
fn done_task_by_number() {
    let temp = TempDir::new().unwrap();

    tdo(&temp).args(["add", "Buy milk"]).assert().success();

    tdo(&temp)
        .args(["done", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task completed: Buy milk"))
        .stdout(predicate::str::contains("#1"));

    tdo(&temp)
        .args(["logbook"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Buy milk"));
}

#[test]
fn done_task_by_fuzzy_name() {
    let temp = TempDir::new().unwrap();

    tdo(&temp).args(["add", "Buy milk"]).assert().success();

    tdo(&temp)
        .args(["done", "milk"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task completed: Buy milk"));
}

#[test]
fn delete_task_by_number() {
    let temp = TempDir::new().unwrap();

    tdo(&temp).args(["add", "Buy milk"]).assert().success();

    tdo(&temp)
        .args(["delete", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task deleted: Buy milk"))
        .stdout(predicate::str::contains("#1"))
        .stdout(predicate::str::contains("tdo trash"))
        .stdout(predicate::str::contains("tdo restore 1"));
}

#[test]
fn restore_task() {
    let temp = TempDir::new().unwrap();

    tdo(&temp).args(["add", "Buy milk"]).assert().success();
    tdo(&temp).args(["delete", "1"]).assert().success();

    tdo(&temp)
        .args(["restore", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task restored: Buy milk"));

    tdo(&temp)
        .args(["inbox"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Buy milk"));
}

#[test]
fn move_task_to_today() {
    let temp = TempDir::new().unwrap();

    tdo(&temp).args(["add", "Buy milk"]).assert().success();

    tdo(&temp)
        .args(["move", "1", "--today"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task moved"))
        .stdout(predicate::str::contains("#1"));

    tdo(&temp)
        .args(["today"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Buy milk"));
}

#[test]
fn move_task_to_project() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "project", "Website"])
        .assert()
        .success();
    tdo(&temp).args(["add", "Fix bug"]).assert().success();

    tdo(&temp)
        .args(["move", "1", "--project", "website"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Project: Website"));
}
