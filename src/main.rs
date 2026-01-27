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
use mongodb_atlas_cli::config::{self, OutputFormat};
use tracing::debug;

use crate::{
    args::{GlobalArgs, LocalArgs},
    commands::command_from_args,
    formatting::Format,
};

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
    let cli = Cli::parse();

    // Split the CLI arguments into global and local arguments.
    let global_args = cli.global_args;
    let cli_arguments: LocalArgs = cli.command.into();

    // Setup logging.
    logging::setup_logging(global_args.debug);

    // Get the format to use for the output.
    let format = get_format(&global_args);

    // Convert the CLI arguments into a command.
    let mut root_command = command_from_args(cli_arguments, format)
        .await
        .context("converting CLI arguments into a command")?;

    // Execute the command.
    root_command.execute().await.context("executing command")?;

    Ok(())
}

/// Get the format to use for the output.
fn get_format(global_args: &GlobalArgs) -> Format {
    // If the format is set, return it.
    if let Some(format) = global_args.format {
        debug!(?format, "Use format provided by --format flag");
        return format;
    }

    // If the format is not set, try to get it from the Atlas CLI config.
    if let Ok(config) = config::load_config(global_args.profile.as_deref()) {
        debug!(profile = ?global_args.profile, "Successfully loaded Atlas CLI config");

        // Check if the output format is set in the config.
        if let Some(format) = config.output {
            debug!(output_format = ?format, "Using output format from Atlas CLI config");
            return match format {
                OutputFormat::Json => Format::Json,
                OutputFormat::Plaintext => Format::Text,
            };
        }
    }

    // If the config is not found, return the default format.
    debug!("No format provided by --format flag or Atlas CLI config, using default format");
    Format::Text
}
