//! This module contains business logic for the commands for the application.
//!
//! The main entry point is the [`command_from_args`] function which converts CLI arguments into a command.
use anyhow::Result;

use crate::{
    args::LocalArgs,
    commands::{delete::Delete, list::List, logs::Logs, start::Start},
    formatting::Format,
};
pub use core::{Command, CommandWithOutput, CommandWithOutputExt};

mod core;
pub mod delete;
pub mod list;
pub mod logs;
pub mod start;

/// Convert CLI arguments into a command.
///
/// This function is the main entry point for the command execution logic.
/// It converts the CLI arguments into a command and returns it.
///
/// The output of the command will be formatted using the provided format and printed to stdout.
pub fn command_from_args(args: LocalArgs, format: Format) -> Result<Box<dyn Command>> {
    match args {
        LocalArgs::Delete(delete_args) => {
            Delete::try_from(delete_args)?.with_print_to_stdout(format)
        }
        LocalArgs::List(list_args) => List::try_from(list_args)?.with_print_to_stdout(format),
        LocalArgs::Logs(logs_args) => Logs::try_from(logs_args)?.with_print_to_stdout(format),
        LocalArgs::Start(start_args) => Start::try_from(start_args)?.with_print_to_stdout(format),
    }
}
