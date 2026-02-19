#![allow(deprecated)]

use assert_cmd::Command;
use assert_fs::TempDir;
use predicates::prelude::*;

fn tdo(temp_dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("tdo").unwrap();
    cmd.env("TDO_DATA_DIR", temp_dir.path());
    cmd
}

#[test]
fn project_lifecycle() {
    let temp = TempDir::new().unwrap();

    // Create a project
    tdo(&temp)
        .args(["create", "project", "Website Redesign"])
        .assert()
        .success();

    // Add tasks to the project
    tdo(&temp)
        .args(["add", "Design mockups", "--project", "website"])
        .assert()
        .success();

    tdo(&temp)
        .args(["add", "Implement frontend", "--project", "website"])
        .assert()
        .success();

    // Verify tasks show under project
    tdo(&temp)
        .args(["show", "project", "website"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Design mockups"))
        .stdout(predicate::str::contains("Implement frontend"));

    // Complete one task
    tdo(&temp)
        .args(["done", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task completed"));

    // Verify completed task in logbook
    tdo(&temp)
        .args(["logbook"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Design mockups"));
}

#[test]
fn trash_restore_workflow() {
    let temp = TempDir::new().unwrap();

    // Add a task
    tdo(&temp).args(["add", "Buy groceries"]).assert().success();

    // Delete it
    tdo(&temp)
        .args(["delete", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task deleted: Buy groceries"));

    // Verify it's in trash
    tdo(&temp)
        .args(["trash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Buy groceries"));

    // Verify it's NOT in inbox
    tdo(&temp)
        .args(["inbox"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Buy groceries").not());

    // Restore it
    tdo(&temp)
        .args(["restore", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task restored: Buy groceries"));

    // Verify it's back in inbox
    tdo(&temp)
        .args(["inbox"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Buy groceries"));

    // Verify trash is empty
    tdo(&temp)
        .args(["trash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Buy groceries").not());
}

#[test]
fn cascade_delete_project() {
    let temp = TempDir::new().unwrap();

    // Create project with tasks
    tdo(&temp)
        .args(["create", "project", "Old Project"])
        .assert()
        .success();

    tdo(&temp)
        .args(["add", "Task A", "--project", "old"])
        .assert()
        .success();

    tdo(&temp)
        .args(["add", "Task B", "--project", "old"])
        .assert()
        .success();

    // Delete the project
    tdo(&temp)
        .args(["remove", "project", "old"])
        .assert()
        .success();

    // Verify tasks are also deleted (in trash)
    tdo(&temp)
        .args(["trash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Task A"))
        .stdout(predicate::str::contains("Task B"));
}
