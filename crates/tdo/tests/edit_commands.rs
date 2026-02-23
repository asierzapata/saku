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

// ============================================================================
// Edit Area Tests
// ============================================================================

#[test]
fn edit_area_renames_area() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work"])
        .assert()
        .success();

    tdo(&temp)
        .args(["edit", "area", "Work", "--new-name", "Professional"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Professional"));

    tdo(&temp)
        .args(["list", "areas"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Professional"))
        .stdout(predicate::str::contains("Work").not());
}

#[test]
fn edit_area_not_found() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["edit", "area", "NonExistent", "--new-name", "NewName"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn edit_area_duplicate_name() {
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
        .args(["edit", "area", "Work", "--new-name", "Personal"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn edit_area_fuzzy_match() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work Stuff"])
        .assert()
        .success();

    tdo(&temp)
        .args(["edit", "area", "work", "--new-name", "Professional"])
        .assert()
        .success();

    tdo(&temp)
        .args(["list", "areas"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Professional"));
}

#[test]
fn edit_area_ambiguous_match() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "area", "Work Projects"])
        .assert()
        .success();
    tdo(&temp)
        .args(["create", "area", "Work Tasks"])
        .assert()
        .success();

    tdo(&temp)
        .args(["edit", "area", "Work", "--new-name", "NewName"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ambiguous"));
}

// ============================================================================
// Edit Project Tests
// ============================================================================

#[test]
fn edit_project_renames_project() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "project", "Website"])
        .assert()
        .success();

    tdo(&temp)
        .args(["edit", "project", "Website", "--new-name", "Blog"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Blog"));

    tdo(&temp)
        .args(["list", "projects"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Blog"))
        .stdout(predicate::str::contains("Website").not());
}

#[test]
fn edit_project_changes_area() {
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
        .args(["create", "project", "Website", "--area", "Work"])
        .assert()
        .success();

    tdo(&temp)
        .args(["edit", "project", "Website", "--area", "Personal"])
        .assert()
        .success();

    tdo(&temp)
        .args(["show", "project", "Website"])
        .assert()
        .success();
}

#[test]
fn edit_project_removes_area() {
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
        .args(["edit", "project", "Website", "--area", ""])
        .assert()
        .success();

    tdo(&temp)
        .args(["list", "projects"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Website"));
}

#[test]
fn edit_project_rename_and_change_area() {
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
        .args(["create", "project", "Website", "--area", "Work"])
        .assert()
        .success();

    tdo(&temp)
        .args([
            "edit",
            "project",
            "Website",
            "--new-name",
            "Blog",
            "--area",
            "Personal",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Blog"));

    tdo(&temp)
        .args(["list", "projects"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Blog"))
        .stdout(predicate::str::contains("Website").not());
}

#[test]
fn edit_project_not_found() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["edit", "project", "NonExistent", "--new-name", "NewName"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn edit_project_duplicate_name() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "project", "Website"])
        .assert()
        .success();
    tdo(&temp)
        .args(["create", "project", "Blog"])
        .assert()
        .success();

    tdo(&temp)
        .args(["edit", "project", "Website", "--new-name", "Blog"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn edit_project_area_not_found() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "project", "Website"])
        .assert()
        .success();

    tdo(&temp)
        .args(["edit", "project", "Website", "--area", "NonExistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn edit_project_fuzzy_match() {
    let temp = TempDir::new().unwrap();

    tdo(&temp)
        .args(["create", "project", "My Website"])
        .assert()
        .success();

    tdo(&temp)
        .args(["edit", "project", "website", "--new-name", "Blog"])
        .assert()
        .success();

    tdo(&temp)
        .args(["list", "projects"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Blog"));
}
