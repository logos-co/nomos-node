mod mixnode;
pub mod nomos;

pub use self::mixnode::MixNode;
pub use nomos::NomosNode;
use tempfile::TempDir;

const LOGS_PREFIX: &str = "__logs";

fn create_tempdir() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn persist_tempdir(tempdir: &mut TempDir, label: &str) {
    println!(
        "{}: persisting directory at {}",
        label,
        tempdir.path().display()
    );
    // we need ownership of the dir to persist it
    let dir = std::mem::replace(tempdir, tempfile::tempdir().unwrap());
    // a bit confusing but `into_path` persists the directory
    let _ = dir.into_path();
}
