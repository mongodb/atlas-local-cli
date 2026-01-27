use clap::{Args, Subcommand};

#[derive(Subcommand)]
#[command(about = "Manage search for local deployments.")]
pub enum Search {
    #[command(subcommand)]
    Indexes(Indexes),
}

#[derive(Subcommand)]
#[command(about = "Manage local search indexes.")]
pub enum Indexes {
    Create(Create),
}

#[derive(Args)]
pub struct Create {
    /// Name of the deployment.
    #[arg(long)]
    pub deployment_name: String,
    /// Flag that indicates whether to watch the command until it completes its execution or the watch times out.
    #[arg(long = "watch", short = 'w', default_value = "false")]
    pub watch: bool,

    /// Username for authenticating to MongoDB.
    #[arg(long = "username", requires = "password")]
    pub username: Option<String>,
    /// Password for authenticating to MongoDB.
    #[arg(long = "password", requires = "username")]
    pub password: Option<String>,

    /// Name of the JSON index configuration file to use.
    ///
    /// To learn about the Atlas Search and Atlas Vector Search index configuration file, see https://dochub.mongodb.org/core/search-index-config-file-atlascli.
    /// To learn about the Atlas Search index syntax and options that you can define in your configuration file, see https://dochub.mongodb.org/core/index-definitions-fts.
    /// To learn about the Atlas Vector Search index syntax and options that you can define in your configuration file, see https://dochub.mongodb.org/core/index-definition-avs.
    #[arg(long, conflicts_with_all = ["database_name", "collection", "index_name"])]
    pub file: Option<String>,

    /// Name of the index.
    #[arg(index = 1, conflicts_with = "file")]
    pub index_name: Option<String>,
    /// Name of the database.
    #[arg(long = "db", conflicts_with = "file")]
    pub database_name: Option<String>,
    /// Name of the collection.
    #[arg(long, conflicts_with = "file")]
    pub collection: Option<String>,
}
