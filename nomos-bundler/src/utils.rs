// STD
use std::path::PathBuf;
use std::process::Command;

pub enum Architecture {
    X86_64,
    Aarch64,
    I686,
}

impl Architecture {
    pub fn from_target_triple(target_triple: &str) -> Self {
        let architecture = target_triple.split("-").next().unwrap();
        Self::from(architecture)
    }
}

impl From<&str> for Architecture {
    fn from(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "x86_64" => Architecture::X86_64,
            "aarch64" => Architecture::Aarch64,
            "i686" => Architecture::I686,
            _ => panic!("Unsupported architecture"),
        }
    }
}

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
pub const fn get_profile(architecture: Architecture) -> &'static str {
    match architecture {
        Architecture::I686 => "x86_64-unknown-linux-gnu/debug",
        _ => "debug",
    }
}

#[cfg(not(debug_assertions))]
pub const fn get_profile(architecture: Architecture) -> &'static str {
    match architecture {
        Architecture::i686 => "x86_64-unknown-linux-gnu/release",
        _ => "release",
    }
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

pub fn get_target_directory_for_current_profile(architecture: Architecture) -> PathBuf {
    let target_directory = get_target_directory();
    let profile = get_profile(architecture);
    target_directory.join(profile)
}
