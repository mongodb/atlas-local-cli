use anyhow::{Context, Result};
use args::Cli;
use clap::Parser;

use crate::{args::LocalArgs, commands::command_from_args, formatting::Format};

mod args;
mod commands;
mod dependencies;
mod formatting;
mod models;
mod table;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse the CLI arguments.
    let cli_arguments: LocalArgs = Cli::parse().into();

    // Convert the CLI arguments into a command.
    // TODO:Format is hardcoded to text for now, we will make it configurable later.
    let mut root_command = command_from_args(cli_arguments, Format::Text)
        .context("converting CLI arguments into a command")?;

    // Execute the command.
    root_command.execute().await.context("executing command")?;

    Ok(())
}
