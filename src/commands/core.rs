//! This module contains the core traits for commands.
//!
//! The goal of this module is to provide a common interface for all commands.
//!
//! There are two main traits:
//! - [`Command`] is a trait for all commands.
//! - [`CommandWithOutput`] is a trait for commands that return an output.
//!
//! There is also a helper trait [`CommandWithOutputExt`] which provides a method to turn
//! a [`CommandWithOutput`] into a [`Command`] that prints the output to stdout.

use anyhow::Result;
use async_trait::async_trait;

use crate::formatting::{Format, Formattable};

/// Trait for all commands.
///
/// A command is a unit of work that can be executed.
#[async_trait]
pub trait Command {
    /// Execute the command
    async fn execute(&mut self) -> Result<()>;
}

/// Trait for commands that return an output.
///
/// The output of a command is the result of the work.
#[async_trait]
pub trait CommandWithOutput {
    type Output;

    /// Execute the command and return the output.
    async fn execute(&mut self) -> Result<Self::Output>;
}

/// Command extensions trait.
pub trait CommandWithOutputExt {
    /// Convert a [`CommandWithOutput`] into a [`Command`] that prints the output to stdout.
    ///
    /// This is a helper method to convert a [`CommandWithOutput`] into a [`Command`] that prints the output to stdout.
    ///
    /// # Arguments
    ///
    /// * `format` - The format to print the output in.
    ///
    /// # Returns
    ///
    /// A [`Command`] that prints the output to stdout.
    fn with_print_to_stdout(self, format: Format) -> Result<Box<dyn Command>>;
}

/// Wrapper command that prints the output of a [`CommandWithOutput`] to stdout.
/// The wrapper implements the [`Command`] trait and prints the output to stdout when executed.
pub struct PrintToStdoutCommand<C, O>
where
    C: CommandWithOutput<Output = O>,
    O: Formattable,
{
    command: C,
    format: Format,
}

impl<C, O> PrintToStdoutCommand<C, O>
where
    C: CommandWithOutput<Output = O> + Send,
    O: Formattable,
{
    pub fn new(command: C, format: Format) -> Self {
        Self { command, format }
    }
}

#[async_trait]
impl<C, O> Command for PrintToStdoutCommand<C, O>
where
    C: CommandWithOutput<Output = O> + Send,
    O: Formattable,
{
    async fn execute(&mut self) -> Result<()> {
        // Execute the command and get the output.
        let output = self.command.execute().await?;

        // Format the output.
        let formatted_output = output.format(self.format)?;

        // Print the formatted output to stdout.
        println!("{}", formatted_output);

        Ok(())
    }
}

impl<C, O> CommandWithOutputExt for C
where
    C: CommandWithOutput<Output = O> + Send + 'static,
    O: Formattable + 'static,
{
    fn with_print_to_stdout(self, format: Format) -> Result<Box<dyn Command>> {
        // Create a new wrapper command that prints the output to stdout.
        Ok(Box::new(PrintToStdoutCommand::new(self, format)))
    }
}
