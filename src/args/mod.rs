//! CLI argument parsing layer.
//!
//! This module provides the CLI interface using clap derive macros.
//! It handles parsing command-line arguments and converting them into structured data types.
//!
//! The business logic layer is [`crate::commands`], which receives these parsed arguments.

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
}

/// List all local deployments.
#[derive(Parser)]
pub struct List;

/// Delete a deployment.
///
/// The command prompts you to confirm the operation when you run the command without the --force option.
//
// Deleting a Local deployment also deletes any local data volumes.
// Deleting a deployment will not remove saved connections from MongoDB for VS Code. This must be done manually. To learn more, see https://www.mongodb.com/docs/mongodb-vscode/connect/#remove-a-connection.
#[derive(Parser)]
pub struct Delete {
    /// Name of the deployment to delete.
    #[arg(index = 1)]
    pub deployment_name: String,

    /// Flag that indicates whether to skip the confirmation prompt before proceeding with the requested action.
    #[arg(long)]
    pub force: bool,
}
