use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: PluginSubCommands,
}

#[derive(Subcommand)]
pub enum PluginSubCommands {
    /// Plugin root subcommand
    Local {
        #[command(subcommand)]
        command: LocalCommand,
    },
}

#[derive(Subcommand)]
pub enum LocalCommand {
    /// The Hello World command
    Hello,
    /// Prints environment variables
    Printenv,
    /// Reads name and prints it
    Stdinreader,
}

impl From<Cli> for LocalCommand {
    fn from(cli: Cli) -> Self {
        match cli.command {
            PluginSubCommands::Local { command } => command,
        }
    }
}
