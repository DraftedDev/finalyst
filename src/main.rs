use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    cli::{Cli, Subcommand},
    config::Config,
    model::Model,
};

mod cli;
mod config;
mod extractor;
mod fetcher;
mod model;
mod rss;
mod utils;

fn main() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(8)
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("Failed to build tokio runtime");

    let cli = Cli::parse();

    let indicatif_tracing = tracing_indicatif::IndicatifLayer::new();

    let level = std::env::var("LOG_LEVEL").unwrap_or("info".to_string());
    tracing_subscriber::registry()
        .with(
            EnvFilter::new("warn")
                .add_directive(format!("finalyst={}", level.as_str()).parse().unwrap()),
        )
        .with(
            fmt::layer()
                .with_writer(indicatif_tracing.get_stderr_writer())
                .with_ansi(true)
                .with_file(false)
                .with_line_number(false)
                .with_thread_ids(false)
                .with_thread_names(false)
                .with_target(false)
                .without_time(),
        )
        .with(indicatif_tracing)
        .init();

    tracing::info!("Using log level: {}", level);

    rt.block_on(async {
        tracing::info!("Loading config file at '{}'...", cli.config);
        let model = Model::new(Config::load(cli.config)).await;

        match cli.subcommand.clone() {
            Subcommand::Analyze(args) => args.run(model).await,
            Subcommand::Reset(args) => args.run(model).await,
        }
    });
}
