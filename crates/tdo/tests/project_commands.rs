#![allow(deprecated)]

use assert_cmd::Command;
use assert_fs::TempDir;
use predicates::prelude::*;

fn tdo(temp_dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("tdo").unwrap();
    cmd.env("TDO_DATA_DIR", temp_dir.path());
    cmd.env("TDO_NO_SYNC", "1");
    cmd
}

#[test]
fn project_new_creates_project() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "project", "Website"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Website"));
}

#[test]
fn project_new_with_area() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .success();

    tdo(&temp)
        .args(["create", "project", "Sprint", "--area", "Work"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sprint"));

    tdo(&temp)
        .args(["list", "projects"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Area: Work"));
}

#[test]
fn project_new_duplicate_returns_error() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "project", "Website"])
        .assert()
        .success();

    tdo(&temp)
        .args(["create", "project", "Website"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn project_list_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["list", "projects"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No projects found"));
}

#[test]
fn project_list_shows_project_info() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .success();
    tdo(&temp)
        .args(["create", "project", "Sprint", "--area", "Work"])
        .assert()
        .success();

    tdo(&temp)
        .args(["list", "projects"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Sprint"))
        .stdout(predicate::str::contains("Work"));
}

#[test]
fn project_delete_removes_project() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "project", "Website"])
        .assert()
        .success();

    tdo(&temp)
        .args(["remove", "project", "Website"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Project deleted: Website"));

    tdo(&temp)
        .args(["list", "projects"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No projects found"));
}

#[test]
fn project_delete_cascades_to_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "project", "Website"])
        .assert()
        .success();
    tdo(&temp)
        .args(["add", "Fix bug", "--project", "website"])
        .assert()
        .success();

    tdo(&temp)
        .args(["remove", "project", "Website"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 task(s) also deleted"));
}

#[test]
fn project_view_shows_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "project", "Website"])
        .assert()
        .success();
    tdo(&temp)
        .args(["add", "Fix bug", "--project", "website"])
        .assert()
        .success();

    tdo(&temp)
        .args(["show", "project", "Website"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Website"))
        .stdout(predicate::str::contains("Fix bug"));
}

#[test]
fn project_view_empty_project() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "project", "Website"])
        .assert()
        .success();

    tdo(&temp)
        .args(["show", "project", "Website"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tasks in project 'Website'"));
}
