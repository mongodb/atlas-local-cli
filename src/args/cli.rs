//! Root command for the CLI.
//!
//! This module contains the root command structure that handles executing the CLI
//! both as a plugin (when invoked via `atlas local`) and as a standalone CLI (when invoked directly as `atlas-local`).
//!
//! The commands are defined in the [`LocalArgs`](super::LocalArgs) enum.
use std::env::args;

use clap::{Args, Subcommand};

use super::LocalArgs;

/// Root command for the CLI.
#[derive(Args)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: PluginSubCommands,
}

/// Implement the Parser trait to allow us to use the Cli struct as a root command.
///
/// This allows us to invoke `Cli::parse()` to parse the CLI arguments.
impl clap::Parser for Cli {}

impl Cli {
    /// Create a new command with the correct binary name based on if we're executing as a plugin or directly.
    fn new_command() -> clap::Command {
        // If the first argument is "local" it means we're executing the executable as a plugin.
        // In that case the binary name should be "atlas" instead of "atlas-local".
        let command = if args().nth(1).as_deref().unwrap_or_default() == "local" {
            "atlas"
        } else {
            "atlas-local"
        };

        clap::Command::new(command).bin_name(command)
    }
}

/// Manually implement the CommandFactory trait to allow us to change the binary name based on execution mode.
///
/// This implementation allows the CLI to dynamically determine its binary name:
/// - If executed as a plugin (`atlas local`), the binary name is "atlas"
/// - If executed directly (`atlas-local`), the binary name is "atlas-local"
impl clap::CommandFactory for Cli {
    fn command() -> clap::Command {
        // This based on what the Parse derive macro generates.
        // The call to `Cli::new_command()` is what's changed
        let __clap_app = Cli::new_command();
        <Self as clap::Args>::augment_args(__clap_app)
    }

    fn command_for_update() -> clap::Command {
        // This based on what the Parse derive macro generates.
        // The call to `Cli::new_command()` is what's changed
        let __clap_app = Cli::new_command();
        <Self as clap::Args>::augment_args_for_update(__clap_app)
    }
}

/// Enum representing the different ways the CLI can be invoked.
///
/// This enum handles the dual nature of the CLI: it can be run as a plugin (`atlas local`)
/// or as a standalone command (`atlas-local`).
#[derive(Subcommand)]
pub enum PluginSubCommands {
    /// The local plugin subcommand
    /// This is the root subcommand when executing the executable as a plugin.
    #[command(hide = true)]
    Local {
        #[command(subcommand)]
        command: LocalArgs,
    },
    /// The local command subcommand
    /// This is the root subcommand when executing the executable directly.
    #[command(flatten)]
    Flat(LocalArgs),
}

/// Convert CLI arguments to local command arguments.
///
/// This allows us to transparently execute the command as a plugin or directly.
/// The conversion extracts the actual command from the plugin wrapper if needed.
impl From<Cli> for LocalArgs {
    fn from(cli: Cli) -> Self {
        match cli.command {
            PluginSubCommands::Local { command } => command,
            PluginSubCommands::Flat(command) => command,
        }
    }
}
