use anyhow::Result;
use async_trait::async_trait;

use crate::{
    args::LocalArgs,
    commands::list::List,
    formatting::{Format, Formattable},
};

pub mod list;

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

pub trait CommandWithOutputExt {
    fn with_print_to_stdout(self, format: Format) -> Result<Box<dyn Command>>;
}

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
        let output = self.command.execute().await?;
        let formatted_output = output.format(self.format)?;
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
        Ok(Box::new(PrintToStdoutCommand::new(self, format)))
    }
}

pub fn command_from_args(args: LocalArgs, format: Format) -> Result<Box<dyn Command>> {
    match args {
        LocalArgs::List(list_args) => List::try_from(list_args)?.with_print_to_stdout(format),
    }
}
