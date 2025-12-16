//! This is the CLI layer. The goal of this module is to provide the CLI interface using
//! clap derive macros.
//!
//! The business logic layer is [`crate::commands`].

use clap::{Parser, Subcommand};

mod cli;

pub use cli::Cli;

// `CLI` handles executing the CLI both as a plugin and as a standalone CLI
// but `LocalCommand` is the root command users will interact with.
//
// We're using `#[command(about = "...")]` here to avoid confusion between the about text and the comments above.
#[derive(Subcommand)]
#[command(about = "Manage local deployments")]
pub enum LocalArgs {
    List(List),
}

/// List all local deployments
#[derive(Parser)]
pub struct List;
