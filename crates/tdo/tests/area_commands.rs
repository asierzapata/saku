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
fn area_new_creates_area() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Work"));
}

#[test]
fn area_new_second_area_does_not_override_first() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .success();

    tdo(&temp)
        .args(["create", "area", "Personal"])
        .assert()
        .success();

    tdo(&temp)
        .args(["list", "areas"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Work"))
        .stdout(predicate::str::contains("Personal"));
}

#[test]
fn area_new_duplicate_name_returns_error() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .success();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn area_list_empty() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["list", "areas"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No areas found"));
}

#[test]
fn area_list_shows_areas_with_counts() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .success();
    tdo(&temp)
        .args(["create", "project", "Proj", "--area", "Work"])
        .assert()
        .success();
    tdo(&temp)
        .args(["add", "My task", "--project", "proj"])
        .assert()
        .success();

    tdo(&temp)
        .args(["list", "areas"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Work"))
        .stdout(predicate::str::contains("1 project"))
        .stdout(predicate::str::contains("1 task"));
}

#[test]
fn area_delete_removes_area() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .success();

    tdo(&temp)
        .args(["remove", "area", "Work"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Area deleted: Work"));

    tdo(&temp)
        .args(["list", "areas"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No areas found"));
}

#[test]
fn area_delete_cascades_projects_and_tasks() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .success();
    tdo(&temp)
        .args(["create", "project", "Proj", "--area", "Work"])
        .assert()
        .success();
    tdo(&temp)
        .args(["add", "My task", "--project", "proj"])
        .assert()
        .success();

    tdo(&temp)
        .args(["remove", "area", "Work"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 project(s) also deleted"))
        .stdout(predicate::str::contains("1 task(s) also deleted"));
}

#[test]
fn area_view_shows_projects() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .success();
    tdo(&temp)
        .args(["create", "project", "Proj", "--area", "Work"])
        .assert()
        .success();
    tdo(&temp)
        .args(["add", "My task", "--project", "proj"])
        .assert()
        .success();

    tdo(&temp)
        .args(["show", "area", "Work"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Work"))
        .stdout(predicate::str::contains("Proj"));
}

#[test]
fn area_view_empty_area() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .success();

    tdo(&temp)
        .args(["show", "area", "Work"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tasks in area 'Work'"));
}
