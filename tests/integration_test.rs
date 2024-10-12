use assert_cmd::prelude::*; // Adds methods like `assert()`
use assert_cmd::Command;
use predicates::prelude::*; // Adds predicates for assertions
use serde_yaml;
use std::fs;
use tempdir::TempDir;

fn setup_temp_dir() -> TempDir {
    let temp_dir = TempDir::new("wdm_test").expect("Failed to create temp dir");
    temp_dir
}

// Helper function to create a fake WordPress plugins directory
fn setup_wp_plugins_dir(temp_dir: &TempDir) -> std::path::PathBuf {
    let wp_plugins_dir = temp_dir.path().join("wp-content/plugins");
    fs::create_dir_all(&wp_plugins_dir).expect("Failed to create wp plugins dir");
    wp_plugins_dir
}

#[test]
fn test_init_command() {
    let temp_dir = setup_temp_dir();

    // Initialize wdm
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.arg("init");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Initialized wdm.yml"));

    // Check that wdm.yml is created
    assert!(temp_dir.path().join("wdm.yml").exists());
}

#[test]
fn test_add_command() {
    let temp_dir = setup_temp_dir();

    // Initialize wdm
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.arg("init");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Initialized wdm.yml"));

    // Add a plugin
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.args(&[
        "add",
        "create-block-theme",
        "--version",
        "2.5.0",
        "--repo",
        "WordPress/create-block-theme",
    ]);
    cmd.assert().success().stdout(predicate::str::contains(
        "Added create-block-theme to wdm.yml",
    ));

    // Check that the dependency is added to wdm.yml
    let wdm_yml = fs::read_to_string(temp_dir.path().join("wdm.yml")).unwrap();
    assert!(wdm_yml.contains("create-block-theme"));
}

#[test]
fn test_install_command() {
    let temp_dir = setup_temp_dir();
    let wp_plugins_dir = setup_wp_plugins_dir(&temp_dir);

    // Initialize wdm
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.arg("init");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Initialized wdm.yml"));

    // Update wdm.yml to set the wordpress_path
    let wdm_yml_path = temp_dir.path().join("wdm.yml");
    let mut wdm_file: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(&wdm_yml_path).unwrap()).unwrap();
    wdm_file["config"]["wordpress_path"] =
        serde_yaml::Value::String(temp_dir.path().to_str().unwrap().to_string());
    fs::write(&wdm_yml_path, serde_yaml::to_string(&wdm_file).unwrap()).unwrap();

    // Add a plugin
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.args(&[
        "add",
        "create-block-theme",
        "--version",
        "2.5.0",
        "--repo",
        "WordPress/create-block-theme",
    ]);
    cmd.assert().success().stdout(predicate::str::contains(
        "Added create-block-theme to wdm.yml",
    ));

    // Install plugins
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.arg("install");
    cmd.assert().success().stdout(predicate::str::contains(
        "Installed create-block-theme 2.5.0",
    ));

    // Check that the plugin is installed
    assert!(wp_plugins_dir.join("create-block-theme").exists());
}

#[test]
fn test_remove_command() {
    let temp_dir = setup_temp_dir();
    let wp_plugins_dir = setup_wp_plugins_dir(&temp_dir);

    // Initialize wdm
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.arg("init");
    cmd.assert().success();

    // Update wdm.yml to set the wordpress_path
    let wdm_yml_path = temp_dir.path().join("wdm.yml");
    let mut wdm_file: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(&wdm_yml_path).unwrap()).unwrap();
    wdm_file["config"]["wordpress_path"] =
        serde_yaml::Value::String(temp_dir.path().to_str().unwrap().to_string());
    fs::write(&wdm_yml_path, serde_yaml::to_string(&wdm_file).unwrap()).unwrap();

    // Add a plugin
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.args(&[
        "add",
        "create-block-theme",
        "--version",
        "2.5.0",
        "--repo",
        "WordPress/create-block-theme",
    ]);
    cmd.assert().success();

    // Install plugins
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.arg("install");
    cmd.assert().success();

    // Remove the plugin
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.args(&["remove", "create-block-theme"]);
    cmd.assert().success().stdout(predicate::str::contains(
        "Removed create-block-theme from wdm.yml",
    ));

    // Check that the plugin is removed from wdm.yml
    let wdm_yml = fs::read_to_string(&wdm_yml_path).unwrap();
    assert!(!wdm_yml.contains("create-block-theme"));

    // Check that the plugin is uninstalled
    assert!(!wp_plugins_dir.join("create-block-theme").exists());
}

#[test]
fn test_version_change_updates_lockfile_and_reinstalls_plugin() {
    let temp_dir = setup_temp_dir();
    let _wp_plugins_dir = setup_wp_plugins_dir(&temp_dir);

    // Initialize wdm
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.arg("init");
    cmd.assert().success();

    // Update wdm.yml to set the wordpress_path
    let wdm_yml_path = temp_dir.path().join("wdm.yml");
    let mut wdm_file: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(&wdm_yml_path).unwrap()).unwrap();
    wdm_file["config"]["wordpress_path"] =
        serde_yaml::Value::String(temp_dir.path().to_str().unwrap().to_string());
    fs::write(&wdm_yml_path, serde_yaml::to_string(&wdm_file).unwrap()).unwrap();

    // Add a plugin with version 1.8.0
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.args(&[
        "add",
        "create-block-theme",
        "--version",
        "1.8.0",
        "--repo",
        "WordPress/create-block-theme",
    ]);
    cmd.assert().success();

    // Install plugins
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.arg("install");
    cmd.assert().success().stdout(predicate::str::contains(
        "Installed create-block-theme 1.8.0",
    ));

    // Check that wdm.lock has version 1.8.0
    let wdm_lock_path = temp_dir.path().join("wdm.lock");
    let wdm_lock: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(&wdm_lock_path).unwrap()).unwrap();
    assert_eq!(wdm_lock["dependencies"][0]["version"], "1.8.0");

    // Update version in wdm.yml to latest
    let mut wdm_file: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(&wdm_yml_path).unwrap()).unwrap();
    wdm_file["dependencies"][0]["version"] = serde_yaml::Value::String("latest".to_string());
    fs::write(&wdm_yml_path, serde_yaml::to_string(&wdm_file).unwrap()).unwrap();

    // Install plugins again
    let mut cmd = Command::cargo_bin("wdm").unwrap();
    cmd.current_dir(&temp_dir);
    cmd.arg("install");
    cmd.assert().success().stdout(predicate::str::contains(
        "Installed create-block-theme 2.5.0",
    ));

    // Check that wdm.lock has updated to version "2.5.0"
    let wdm_lock: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(&wdm_lock_path).unwrap()).unwrap();
    assert_eq!(wdm_lock["dependencies"][0]["version"], "2.5.0");

    // Ensure that the version has changed
    assert_ne!(wdm_lock["dependencies"][0]["version"], "1.8.0");
}
