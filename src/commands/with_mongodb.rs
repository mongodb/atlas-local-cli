use anyhow::{Context, Result};
use bollard::Docker;
use mongodb::{
    Client,
    options::{ClientOptions, ConnectionString, Credential},
};

pub trait TryFromWithMongodbClient<T>: Sized {
    fn try_from_with_mongodb(
        value: T,
        client: Result<Client, TryToGetMongodbClientError>,
    ) -> Result<Self>;
}

#[derive(Debug, thiserror::Error)]
pub enum TryToGetMongodbClientError {
    #[error("Failed to connect to docker: {0}")]
    ConnectingToDocker(anyhow::Error),
    #[error("Failed to get connection string for local deployment: {0}")]
    GettingConnectionString(anyhow::Error),
    #[error("Failed to create mongodb client: {0}")]
    CreatingMongodbClient(anyhow::Error),
}
pub trait WithMongodbClientForLocalDeployment<Args, FDeployment, FUsername, FPassword>:
    Sized
where
    FDeployment: Fn(&Args) -> String,
    FUsername: Fn(&Args) -> Option<String>,
    FPassword: Fn(&Args) -> Option<String>,
{
    async fn with_mongodb_client_for_local_deployment(
        args: Args,
        local_deployment_name_fn: FDeployment,
        username_fn: FUsername,
        password_fn: FPassword,
    ) -> Result<Self>;
}

impl<Args, T, FDeployment, FUsername, FPassword>
    WithMongodbClientForLocalDeployment<Args, FDeployment, FUsername, FPassword> for T
where
    T: TryFromWithMongodbClient<Args>,
    FDeployment: Fn(&Args) -> String,
    FUsername: Fn(&Args) -> Option<String>,
    FPassword: Fn(&Args) -> Option<String>,
{
    async fn with_mongodb_client_for_local_deployment(
        args: Args,
        local_deployment_name_fn: FDeployment,
        username_fn: FUsername,
        password_fn: FPassword,
    ) -> Result<Self> {
        // Extract the local deployment name, username, and password from the arguments.
        let local_deployment_name = local_deployment_name_fn(&args);
        let username = username_fn(&args);
        let password = password_fn(&args);

        // Try to get a mongodb client for the local deployment.
        let client_result =
            try_get_mongodb_client_for_local_deployment(local_deployment_name, username, password)
                .await;

        // Finally create a new instance of the command using the arguments and the mongodb client.
        Self::try_from_with_mongodb(args, client_result)
    }
}

async fn try_get_mongodb_client_for_local_deployment(
    local_deployment_name: String,
    username: Option<String>,
    password: Option<String>,
) -> Result<Client, TryToGetMongodbClientError> {
    // Connect to docker and create a new client.
    let client = atlas_local::Client::new(
        Docker::connect_with_defaults()
            .context("connecting to docker")
            .map_err(TryToGetMongodbClientError::ConnectingToDocker)?,
    );

    // Get the connection string for the local deployment.
    let connection_string = client
        .get_connection_string(local_deployment_name)
        .await
        .context("getting connection string")
        .map_err(TryToGetMongodbClientError::GettingConnectionString)?;

    // Override the username and password if they are provided.
    let mut db_connection_string = ConnectionString::parse(connection_string)
        .context("parsing connection string")
        .map_err(TryToGetMongodbClientError::GettingConnectionString)?;

    if username.is_some() || password.is_some() {
        let mut credential = Credential::default();
        credential.username = username;
        credential.password = password;
        db_connection_string.credential.replace(credential);
    }

    // Create a new mongodb client from the connection string.
    let client_options = ClientOptions::parse(db_connection_string)
        .await
        .context("parsing connection string")
        .map_err(TryToGetMongodbClientError::CreatingMongodbClient)?;

    Client::with_options(client_options)
        .context("creating mongodb client")
        .map_err(TryToGetMongodbClientError::CreatingMongodbClient)
}
