use std::time::Duration;

use clap::Parser;

use crate::model::Model;

/// Interface for the Finalyst Market Analysis Tool.
#[derive(Parser)]
pub struct Cli {
    /// Path to the configuration file.
    #[clap(short = 'c', long = "config", default_value = "config.json")]
    pub config: String,
    /// Subcommand to run.
    #[clap(subcommand)]
    pub subcommand: Subcommand,
}

/// Subcommands for the Finalyst Market Analysis Tool.
#[derive(Clone, Debug, Parser)]
pub enum Subcommand {
    /// Analyze the market data.
    Analyze(AnalyzeArgs),
    /// Reset the database.
    Reset(ResetArgs),
    /// Collect and process RSS feeds without running the analysis.
    Collect(CollectArgs),
}

/// Arguments for the `launch` subcommand.
#[derive(Clone, Debug, Parser)]
pub struct AnalyzeArgs {}

impl AnalyzeArgs {
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
        model.collect(self.limit.unwrap_or(usize::MAX)).await;
    }
}
