pub mod network;
pub mod node;
pub mod output_processors;
pub mod overlay;
pub mod runner;
pub mod settings;
pub mod streaming;
pub mod warding;

static CONFIGURATION: once_cell::sync::OnceCell<settings::SimulationSettings> = once_cell::sync::OnceCell::new();