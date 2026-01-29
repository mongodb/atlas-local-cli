//! Root command for the CLI.
//!
//! This module contains the root command structure that handles executing the CLI
//! both as a plugin (when invoked via `atlas local`) and as a standalone CLI (when invoked directly as `atlas-local`).
//!
//! The commands are defined in the [`LocalArgs`](super::LocalArgs) enum.
use std::env::args;

use clap::{Args, Subcommand};

use crate::formatting::Format;

use super::LocalArgs;

/// Manage local deployments
#[derive(Args)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(flatten)]
    pub global_args: GlobalArgs,

    #[command(subcommand)]
    pub command: PluginSubCommands,
}

/// Implement the Parser trait to allow us to use the Cli struct as a root command.
///
/// This allows us to invoke `Cli::parse()` to parse the CLI arguments.
impl clap::Parser for Cli {}

impl Cli {
    /// Create a new command with the correct binary name based on if we're executing as a plugin or directly.
    ///
    /// Setting the binary name changes the usage string in the help text.
    /// e.g. if the binary name is "atlas", the usage string will be "Usage: atlas <COMMAND>".
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

/// Manually implement the CommandFactory trait to change the help text format based on execution mode.
///
/// The main goal of this implementation is to ensure that the usage string in the help text aligns with the execution mode.
///
/// This implementation allows the CLI to dynamically determine its binary name:
/// - If executed as a plugin (`atlas local`), the binary name is "atlas", and the usage string is "Usage: atlas local <COMMAND>".
/// - If executed directly (`atlas-local`), the binary name is "atlas-local", and the usage string is "Usage: atlas-local <COMMAND>".
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

#[derive(Args)]
#[command(rename_all = "camelCase")]
pub struct GlobalArgs {
    /// Enable debug logging.
    ///
    /// Setting this flag will set the log level to debug and only show logs from this crate.
    ///
    /// The log level can also be overridden by setting the `ATLAS_LOCAL_LOG` environment variable.
    /// If the `ATLAS_LOCAL_LOG_ALL` environment variable is set, it will show logs from all crates at the specified level.
    ///
    /// The log level can also be overridden by setting the `ATLAS_LOCAL_LOG_ALL` environment variable.
    /// If the `ATLAS_LOCAL_LOG_ALL` environment variable is set, it will show logs from all crates at the specified level.
    ///
    /// The log level can also be overridden by setting the `ATLAS_LOCAL_LOG_ALL` environment variable.
    #[arg(global = true, hide = true, long, short = 'D', default_value = "false")]
    pub debug: bool,

    /// Output format.
    #[arg(global = true, long = "output", short = 'o')]
    pub format: Option<Format>,

    /// Name of the profile to use from your configuration file.
    /// To learn about profiles for the Atlas CLI, see https://dochub.mongodb.org/core/atlas-cli-save-connection-settings.
    #[arg(global = true, long, short = 'P')]
    pub profile: Option<String>,
}

/// Enum representing the different ways the CLI can be invoked.
///
/// This enum handles the dual nature of the CLI: it can be run as a plugin (`atlas local`)
/// or as a standalone command (`atlas-local`).
#[derive(Subcommand)]
pub enum PluginSubCommands {
    /// Manage local deployments.
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
impl From<PluginSubCommands> for LocalArgs {
    fn from(command: PluginSubCommands) -> Self {
        match command {
            PluginSubCommands::Local { command } => command,
            PluginSubCommands::Flat(command) => command,
        }
    }
}
