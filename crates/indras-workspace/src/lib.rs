pub mod state;
pub mod components;
pub mod bridge;
pub mod scripting;
pub mod mock_artifacts;

/// Whether `--mock` was passed on the command line.
pub static MOCK_ARTIFACTS: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
