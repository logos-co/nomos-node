use clap::Parser;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use simulations::{
    config::Config,
    node::{
        carnot::{CarnotNode, CARNOT_LEADER_STEPS},
        StepTime,
    },
    overlay::{flat::FlatOverlay, Overlay},
    runner::{ConsensusRunner, LayoutNodes},
};

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path for a yaml-encoded network config file
    config: std::path::PathBuf,
    #[arg(long, default_value_t = String::from("flat"))]
    overlay_type: String,
    #[arg(long, default_value_t = String::from("carnot"))]
    node_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
enum OverlayType {
    Flat,
}

impl TryFrom<&str> for OverlayType {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "flat" => Ok(Self::Flat),
            _ => Err(format!("Unknown overlay type: {}", value)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum NodeType {
    Carnot,
}

impl TryFrom<&str> for NodeType {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "carnot" => Ok(Self::Carnot),
            _ => Err(format!("Unknown overlay type: {}", value)),
        }
    }
}

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Args {
        config,
        overlay_type,
        node_type,
    } = Args::parse();
    let overlay_type = OverlayType::try_from(overlay_type.as_str())?;
    let node_type = NodeType::try_from(node_type.as_str())?;

    let report = match (overlay_type, node_type) {
        (OverlayType::Flat, NodeType::Carnot) => {
            let cfg = serde_json::from_reader::<_, Config<CarnotNode, FlatOverlay>>(
                std::fs::File::open(config)?,
            )?;
            let overlay = FlatOverlay::new(cfg.overlay_settings);
            let node_ids = (0..cfg.node_count).collect::<Vec<_>>();
            let mut rng = thread_rng();
            let layout = overlay.layout(&node_ids, &mut rng);
            let leaders = overlay.leaders(&node_ids, 1, &mut rng).collect();

            let carnot_steps: Vec<_> = CARNOT_LEADER_STEPS
                .iter()
                .copied()
                .map(|step| {
                    (
                        LayoutNodes::Leader,
                        step,
                        Box::new(|times: &[StepTime]| *times.iter().max().unwrap())
                            as Box<dyn Fn(&[StepTime]) -> StepTime>,
                    )
                })
                .collect();

            let mut runner: simulations::runner::ConsensusRunner<CarnotNode> =
                ConsensusRunner::new(&mut rng, layout, leaders, cfg.node_settings);
            runner.run(&carnot_steps)
        }
    };

    println!("{:?}", report);
    Ok(())
}
