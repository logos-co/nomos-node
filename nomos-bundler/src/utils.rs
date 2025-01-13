// STD
use std::path::PathBuf;
use std::process::Command;

fn cargo_metadata() -> serde_json::Value {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version=1")
        .output()
        .expect("Failed to execute `cargo metadata`");

    serde_json::from_slice(&output.stdout).expect("Failed to parse `cargo metadata` output")
}

pub fn get_target_directory() -> PathBuf {
    let metadata = cargo_metadata();
    let target_directory = metadata["target_directory"]
        .as_str()
        .expect("Failed to get target directory");
    PathBuf::from(target_directory)
}

#[cfg(debug_assertions)]
pub const fn get_profile() -> &'static str {
    "debug"
}

#[cfg(not(debug_assertions))]
pub const fn get_profile() -> &'static str {
    "release"
}

pub fn get_project_identifier(crate_name: &str) -> String {
    format!("com.nomos.{crate_name}")
}

pub fn get_workspace_root() -> PathBuf {
    let metadata = cargo_metadata();
    let workspace_root = metadata["workspace_root"]
        .as_str()
        .expect("Failed to get workspace root");
    PathBuf::from(workspace_root)
}

pub fn get_target_directory_for_current_profile() -> PathBuf {
    let target_directory = get_target_directory();
    let profile = get_profile();
    target_directory.join(profile)
}
