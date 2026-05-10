use std::io::{IsTerminal, stderr};

use clap::Parser;
use kdam::term;
use tracing::level_filters::LevelFilter;

use crate::{
    cli::{Cli, Subcommand},
    model::{Model, ModelConfig},
};

mod cli;
mod model;
mod rss;
mod utils;

fn main() {
    tracing_subscriber::fmt()
        .with_ansi(true)
        .with_file(false)
        .with_line_number(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .without_time()
        .with_max_level(LevelFilter::INFO)
        .init();

    let rt = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(8)
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("Failed to build tokio runtime");

    let cli = Cli::parse();

    term::init(stderr().is_terminal());

    rt.block_on(async {
        tracing::info!("Loading model from config at '{}'...", cli.config);
        let model = Model::new(ModelConfig::load(cli.config)).await;

        match cli.subcommand.clone() {
            Subcommand::Launch(args) => args.run(model).await,
            Subcommand::Reset(args) => args.run(model).await,
            Subcommand::Collect(args) => args.run(model).await,
        }
    });
}
