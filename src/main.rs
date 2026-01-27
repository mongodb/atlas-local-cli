//! Main entry point for the Atlas Local CLI application.
//!
//! This module handles the application's startup flow:
//! 1. Parses CLI arguments using clap
//! 2. Converts CLI arguments into executable commands
//! 3. Executes the commands and handles their output
//!
//! The application can be run either as a standalone CLI (`atlas-local`) or as an Atlas CLI plugin (`atlas local`).

use anyhow::{Context, Result};
use args::Cli;
use clap::Parser;

use crate::{args::LocalArgs, commands::command_from_args, formatting::Format};

mod args;
mod commands;
mod dependencies;
mod formatting;
mod interaction;
mod logging;
mod models;
mod table;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse the CLI arguments.
    let cli_arguments: LocalArgs = Cli::parse().into();

    // Setup logging.
    logging::setup_logging();

    // Convert the CLI arguments into a command.
    // TODO:Format is hardcoded to text for now, we will make it configurable later.
    let mut root_command = command_from_args(cli_arguments, Format::Text)
        .await
        .context("converting CLI arguments into a command")?;

    // Execute the command.
    root_command.execute().await.context("executing command")?;

    Ok(())
}
