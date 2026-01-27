//! CLI argument parsing layer.
//!
//! This module provides the CLI interface using clap derive macros.
//! It handles parsing command-line arguments and converting them into structured data types.
//!
//! The business logic layer is [`crate::commands`], which receives these parsed arguments.

use std::{path::PathBuf, time::Duration};

use atlas_local::models::MongoDBVersion;
use clap::{Parser, Subcommand, ValueEnum};

mod cli;
pub mod search;

pub use cli::Cli;
pub use search::{Indexes, Search};

/// Manage local deployments.
#[derive(Subcommand)]
#[command(about = "Manage local deployments")]
pub enum LocalArgs {
    #[command(alias = "rm")]
    Delete(Delete),
    #[command(alias = "ls")]
    List(List),
    #[command(alias = "log")]
    Logs(Logs),
    Setup(Setup),
    Start(Start),
    #[command(alias = "pause")]
    Stop(Stop),
    Connect(Connect),
    #[command(subcommand)]
    Search(Search),
}

/// List all local deployments.
#[derive(Parser)]
pub struct List;

/// Delete a deployment.
///
/// The command prompts you to confirm the operation when you run the command without the --force option.
///
/// Deleting a Local deployment also deletes any local data volumes.
/// Deleting a deployment will not remove saved connections from MongoDB for VS Code. This must be done manually. To learn more, see https://www.mongodb.com/docs/mongodb-vscode/connect/#remove-a-connection.
#[derive(Parser)]
pub struct Delete {
    /// Name of the deployment to delete.
    #[arg(index = 1)]
    pub deployment_name: String,

    /// Flag that indicates whether to skip the confirmation prompt before proceeding with the requested action.
    #[arg(long)]
    pub force: bool,
}

/// Get deployment logs.
#[derive(Parser)]
pub struct Logs {
    /// Name of the deployment to get logs from.
    #[arg(index = 1)]
    pub deployment_name: String,
}

/// Start a deployment.
#[derive(Parser)]
pub struct Start {
    /// Name of the deployment to start.
    #[arg(index = 1)]
    pub deployment_name: String,

    /// Flag that indicates whether to wait for the deployment to be healthy before returning.
    #[arg(long, default_value = "false")]
    pub wait_for_healthy: bool,

    /// Timeout for the wait for healthy deployment.
    /// The format is a number followed by a unit. Relevant time units are ms, s, m, h
    /// When no unit is provided, the unit is assumed to be seconds.
    #[arg(long, default_value = "10m", value_parser = parse_duration)]
    pub wait_for_healthy_timeout: Duration,
}

/// Create a local deployment.
///
/// To learn more about local atlas deployments, see https://www.mongodb.com/docs/atlas/cli/current/atlas-cli-deploy-local/
#[derive(Parser)]
pub struct Setup {
    /// Name of the deployment that you want to set up.
    #[arg(index = 1)]
    pub deployment_name: Option<String>,

    /// MongoDB version to use for the deployment.
    ///
    /// Expected format: <major>[.<minor>[.<patch>]] or 'latest'.
    /// Some examples: 8, 8.2, 8.2.1, latest
    #[arg(long, value_parser = parse_mdb_version)]
    pub mdb_version: Option<MongoDBVersion>,

    /// Port that the MongoDB server listens to for client connections.
    ///
    /// The port must be between 1 and 65535.
    #[arg(long)]
    pub port: Option<u16>,

    /// Flag that indicates whether the LOCAL deployment port binding should happen for all IPs or only for the localhost interface 127.0.0.1.
    ///
    /// The default is false.
    #[arg(long, default_value = "false")]
    pub bind_ip_all: bool,

    /// Flag that uses a folder to be mapped into LOCAL deployment for initialization
    ///
    /// The folder must exist and be a directory.
    #[arg(long, value_parser = parse_directory)]
    pub initdb: Option<PathBuf>,

    /// Flag that indicates whether to skip the confirmation prompt before proceeding with the requested action.
    ///
    /// The default is false.
    #[arg(long, default_value = "false")]
    pub force: bool,

    /// Flag that indicates whether to load sample data into the deployment.
    ///
    /// The default is false.
    #[arg(long)]
    pub load_sample_data: Option<bool>,

    /// Username for authenticating to MongoDB.
    #[arg(long)]
    pub username: Option<String>,

    /// Password for the user.
    #[arg(long)]
    pub password: Option<String>,

    /// Alternative docker image to use for the deployment.
    ///
    /// The default is the official MongoDB Atlas Local image.
    /// To learn more about the official MongoDB Atlas Local image, see https://hub.docker.com/r/mongodb/mongodb-atlas-local
    #[arg(long)]
    pub image: Option<String>,

    /// Flag that indicates whether to skip the pull image step.
    ///
    /// This will prevent the CLI from pulling the latest MongoDB Atlas Local image. Use with caution as you might end up with an outdated image.
    ///
    /// The default is false.
    #[arg(long, default_value = "false")]
    pub skip_pull_image: bool,

    /// Method for connecting to the deployment after setup.
    ///
    /// If not provided, the user will be prompted to select a connection method.
    #[arg(long)]
    pub connect_with: Option<ConnectWith>,
}

/// Stop (pause) a deployment.
#[derive(Parser)]
pub struct Stop {
    /// Name of the deployment to stop.
    #[arg(index = 1)]
    pub deployment_name: String,
}

/// Connect to a deployment.
#[derive(Parser)]
pub struct Connect {
    /// Name of the deployment that you want to connect to.
    #[arg(index = 1)]
    pub deployment_name: String,

    /// Method for connecting to the deployment.
    #[arg(long)]
    pub connector: ConnectWith,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValueEnum)]
pub enum ConnectWith {
    #[value(name = "compass")]
    Compass,
    #[value(name = "mongosh")]
    Mongosh,
    #[value(name = "vscode")]
    VsCode,
    #[value(name = "connectionString")]
    ConnectionString,
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    duration_str::parse(s).map_err(|e| e.to_string())
}

fn parse_directory(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);

    // Check if the path exists.
    if !path.exists() {
        return Err(format!("The directory {} does not exist", s));
    }

    // Check if the path is a directory.
    if !path.is_dir() {
        return Err(format!("The path {} is not a directory", s));
    }

    // Return the path as a PathBuf.
    Ok(path)
}

fn parse_mdb_version(s: &str) -> Result<MongoDBVersion, String> {
    MongoDBVersion::try_from(s).map_err(|e| e.to_string())
}
