//! CLI argument parsing layer.
//!
//! This module provides the CLI interface using clap derive macros.
//! It handles parsing command-line arguments and converting them into structured data types.
//!
//! The business logic layer is [`crate::commands`], which receives these parsed arguments.

use std::time::Duration;

use clap::{Parser, Subcommand};

mod cli;

pub use cli::Cli;

/// Root command enum for local deployment management.
///
/// `LocalArgs` is the root command that users will interact with.
/// It contains all available subcommands for managing local deployments.
#[derive(Subcommand)]
#[command(about = "Manage local deployments")]
pub enum LocalArgs {
    #[command(alias = "rm")]
    Delete(Delete),
    #[command(alias = "ls")]
    List(List),
    #[command(alias = "log")]
    Logs(Logs),
    Start(Start),
}

/// List all local deployments.
#[derive(Parser)]
pub struct List;

/// Delete a deployment.
///
/// The command prompts you to confirm the operation when you run the command without the --force option.
///
/// Deleting a Local deployment also deletes any local data volumes.
/// Deleting a deployment will not remove saved connections from MongoDB for VS Code. This must be done manually. To learn more, see https://www.mongodb.com/docs/mongodb-vscode/connect/#remove-a-connection.
#[derive(Parser)]
pub struct Delete {
    /// Name of the deployment to delete.
    #[arg(index = 1)]
    pub deployment_name: String,

    /// Flag that indicates whether to skip the confirmation prompt before proceeding with the requested action.
    #[arg(long)]
    pub force: bool,
}

/// Get deployment logs.
#[derive(Parser)]
pub struct Logs {
    /// Name of the deployment to get logs from.
    #[arg(index = 1)]
    pub deployment_name: String,
}

/// Start a deployment.
#[derive(Parser)]
pub struct Start {
    /// Name of the deployment to start.
    #[arg(index = 1)]
    pub deployment_name: String,

    /// Flag that indicates whether to wait for the deployment to be healthy before returning.
    #[arg(long, default_value = "false")]
    pub wait_for_healthy: bool,

    /// Timeout for the wait for healthy deployment.
    /// The format is a number followed by a unit. Relevant time units are ms, s, m, h
    /// When no unit is provided, the unit is assumed to be seconds.
    #[arg(long, default_value = "10m", value_parser = parse_duration)]
    pub wait_for_healthy_timeout: Duration,
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    duration_str::parse(s).map_err(|e| e.to_string())
}
