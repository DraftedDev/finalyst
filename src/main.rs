use std::io::{IsTerminal, stderr};

use clap::Parser;
use kdam::term;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    cli::{Cli, Subcommand},
    model::{Model, ModelConfig},
};

mod cli;
mod finance;
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

    tracing_subscriber::registry()
        .with(
            EnvFilter::new(&cli.level)
                .add_directive("hyper=error".parse().unwrap())
                .add_directive("reqwest=warn".parse().unwrap())
                .add_directive("h2=error".parse().unwrap())
                .add_directive("tower=error".parse().unwrap())
                .add_directive("rustls=error".parse().unwrap()),
        )
        .with(
            fmt::layer()
                .with_ansi(true)
                .with_file(false)
                .with_line_number(false)
                .with_thread_ids(false)
                .with_thread_names(false)
                .without_time(),
        )
        .init();

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
