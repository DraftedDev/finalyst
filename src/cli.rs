use std::time::Duration;

use clap::Parser;

use crate::model::Model;

/// Interface for the Finalyst Market Analysis Tool.
#[derive(Parser)]
pub struct Cli {
    /// Path to the configuration file.
    #[clap(short = 'c', long = "config", default_value = "config.toml")]
    pub config: String,
    #[clap(subcommand)]
    pub subcommand: Subcommand,
}

/// Subcommands for the Finalyst Market Analysis Tool.
#[derive(Clone, Debug, Parser)]
pub enum Subcommand {
    /// Launch the analysis tool.
    Launch(LaunchArgs),
    /// Reset the database.
    Reset(ResetArgs),
    /// Collect and process RSS feeds without running the analysis.
    Collect(CollectArgs),
}

/// Arguments for the `launch` subcommand.
#[derive(Clone, Debug, Parser)]
pub struct LaunchArgs {}

impl LaunchArgs {
    pub async fn run(self, model: Model) {
        let result = model.analyze().await;
        println!("--------------------[ RESULT ]--------------------");
        println!("{}", result);
        println!("--------------------------------------------------");
    }
}

/// Arguments for the `reset` subcommand.
#[derive(Clone, Debug, Parser)]
pub struct ResetArgs {}

impl ResetArgs {
    pub async fn run(self, model: Model) {
        for i in [3, 2, 1] {
            tracing::info!("Resetting database in {}...", i);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        tracing::info!("Resetting database...");
        model.reset().await;
    }
}

/// Arguments for the `collect` subcommand.
#[derive(Clone, Debug, Parser)]
pub struct CollectArgs {
    /// Optionally limit the number of RSS feeds to collect.
    #[clap(short = 'l', long = "limit")]
    limit: Option<usize>,
}

impl CollectArgs {
    pub async fn run(self, model: Model) {
        tracing::info!("Collecting RSS feeds...");
        model.fetch(self.limit).await;
    }
}
