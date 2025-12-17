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
    List(List),
}

/// List all local deployments.
#[derive(Parser)]
pub struct List;
