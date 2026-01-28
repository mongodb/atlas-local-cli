//! This module contains business logic for the commands for the application.
//!
//! The main entry point is the [`command_from_args`] function which converts CLI arguments into a command.
use anyhow::Result;

use crate::{
    args::{Indexes, LocalArgs, Search},
    commands::{
        connect::Connect, delete::Delete, list::List, logs::Logs, setup::Setup, start::Start,
        stop::Stop, with_mongodb::WithMongodbClientForLocalDeployment,
    },
    formatting::Format,
};
pub use core::{Command, CommandWithOutput, CommandWithOutputExt};

pub mod connect;
mod connectors;
mod core;
pub mod delete;
pub mod list;
pub mod logs;
pub mod search;
pub mod setup;
pub mod start;
pub mod stop;
mod validators;
mod with_mongodb;

/// Convert CLI arguments into a command.
///
/// This function is the main entry point for the command execution logic.
/// It converts the CLI arguments into a command and returns it.
///
/// The output of the command will be formatted using the provided format and printed to stdout.
pub async fn command_from_args(args: LocalArgs, format: Format) -> Result<Box<dyn Command>> {
    match args {
        LocalArgs::Delete(delete_args) => {
            Delete::try_from(delete_args)?.with_print_to_stdout(format)
        }
        LocalArgs::List(list_args) => List::try_from(list_args)?.with_print_to_stdout(format),
        LocalArgs::Logs(logs_args) => Logs::try_from(logs_args)?.with_print_to_stdout(format),
        LocalArgs::Setup(setup_args) => Setup::try_from(setup_args)?.with_print_to_stdout(format),
        LocalArgs::Start(start_args) => Start::try_from(start_args)?.with_print_to_stdout(format),
        LocalArgs::Stop(stop_args) => Stop::try_from(stop_args)?.with_print_to_stdout(format),
        LocalArgs::Connect(connect_args) => {
            Connect::try_from(connect_args)?.with_print_to_stdout(format)
        }
        LocalArgs::Search(search_args) => search_command_from_args(search_args, format).await,
    }
}

async fn search_command_from_args(args: Search, format: Format) -> Result<Box<dyn Command>> {
    match args {
        Search::Indexes(indexes_args) => match indexes_args {
            Indexes::Create(create_args) => {
                search::create::Create::with_mongodb_client_for_local_deployment(
                    create_args,
                    |args| args.deployment_name.clone(),
                    |args| args.username.clone(),
                    |args| args.password.clone(),
                )
                .await?
                .with_print_to_stdout(format)
            }
            Indexes::Describe(describe_args) => {
                search::describe::Describe::with_mongodb_client_for_local_deployment(
                    describe_args,
                    |args| args.deployment_name.clone(),
                    |args| args.username.clone(),
                    |args| args.password.clone(),
                )
                .await?
                .with_print_to_stdout(format)
            }
            Indexes::List(list_args) => {
                search::list::List::with_mongodb_client_for_local_deployment(
                    list_args,
                    |args| args.deployment_name.clone(),
                    |args| args.username.clone(),
                    |args| args.password.clone(),
                )
                .await?
                .with_print_to_stdout(format)
            }
            Indexes::Delete(delete_args) => {
                search::delete::Delete::with_mongodb_client_for_local_deployment(
                    delete_args,
                    |args| args.deployment_name.clone(),
                    |args| args.username.clone(),
                    |args| args.password.clone(),
                )
                .await?
                .with_print_to_stdout(format)
            }
        },
    }
}
