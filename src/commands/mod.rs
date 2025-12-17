//! This module contains business logic for the commands for the application.
//!
//! The main entry point is the [`command_from_args`] function which converts CLI arguments into a command.
use anyhow::Result;

use crate::{args::LocalArgs, commands::list::List, formatting::Format};
pub use core::{Command, CommandWithOutput, CommandWithOutputExt};

mod core;
pub mod list;

/// Convert CLI arguments into a command.
///
/// This function is the main entry point for the command execution logic.
/// It converts the CLI arguments into a command and returns it.
///
/// The output of the command will be formatted using the provided format and printed to stdout.
pub fn command_from_args(args: LocalArgs, format: Format) -> Result<Box<dyn Command>> {
    match args {
        LocalArgs::List(list_args) => List::try_from(list_args)?.with_print_to_stdout(format),
    }
}
