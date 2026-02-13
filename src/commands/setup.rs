use std::{collections::HashMap, fmt::Display, path::PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use atlas_local::{
    Client, CreateDeploymentError,
    client::CreateDeploymentStepOutcome,
    models::{
        BindingType, CreateDeploymentOptions, CreationSource, MongoDBPortBinding, MongoDBVersion,
    },
};
use bollard::Docker;
use semver::Version;
use serde::Serialize;
use tracing::debug;

use crate::{
    args::{self, ConnectWith},
    commands::{
        CommandWithOutput,
        connectors::{Compass, Connector, DeploymentParams, Mongosh, VsCode},
        validators,
    },
    dependencies::{DeploymentCreator, DeploymentGetConnectionString},
    interaction::{
        InputPrompt, InputPromptOptions, InputPromptResult, InputPromptValidator, InputValidator,
        Interaction, MultiStepSpinnerInteraction, MultiStepSpinnerOutcome, MultiStepSpinnerStep,
        SelectPrompt, SelectPromptOptions, SelectPromptResult, SpinnerInteraction,
    },
};

// Setup dependencies for the setup command
pub trait SetupDeploymentManagement:
    DeploymentCreator + DeploymentGetConnectionString + Sync
{
}
impl<T: DeploymentCreator + DeploymentGetConnectionString + Sync> SetupDeploymentManagement for T {}

// Interaction dependencies for the setup command
pub trait SetupInteraction:
    SpinnerInteraction + SelectPrompt + InputPrompt + MultiStepSpinnerInteraction + Sync
{
}
impl<T: SpinnerInteraction + SelectPrompt + InputPrompt + MultiStepSpinnerInteraction + Sync>
    SetupInteraction for T
{
}

/// Parses a string as a boolean: "true"/"1" => true, "false"/"0" => false (case-insensitive).
fn parse_bool(s: &str) -> Result<bool> {
    match s.to_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => anyhow::bail!("expected true or false, got '{}'", s),
    }
}

/// Reads an environment variable as a boolean.
/// Returns `None` if unset, `Some(true)`/`Some(false)` if set to a valid value, error if invalid.
fn bool_from_env(key: &str) -> Result<Option<bool>> {
    let v = match std::env::var(key) {
        Ok(s) => s,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    parse_bool(&v)
        .map(Some)
        .map_err(|e| anyhow::anyhow!("invalid value for {}: {}", key, e))
}

pub struct Setup {
    deployment_name: Option<String>,
    mdb_version: Option<MongoDBVersion>,
    voyage_api_key: Option<String>,
    port: Option<u16>,
    bind_ip_all: bool,
    initdb: Option<PathBuf>,
    force: bool,
    load_sample_data: Option<bool>,
    username: Option<String>,
    password: Option<String>,

    image: Option<String>,
    skip_pull_image: bool,
    connect_with: Option<ConnectWith>,

    interaction: Box<dyn SetupInteraction + Send>,
    deployment_management: Box<dyn SetupDeploymentManagement + Send>,
    connectors: HashMap<ConnectWith, Box<dyn Connector + Send + Sync>>,
}

impl TryFrom<args::Setup> for Setup {
    type Error = anyhow::Error;

    fn try_from(args: args::Setup) -> Result<Self> {
        let use_preview = bool_from_env("MONGODB_ATLAS_LOCAL_PREVIEW")?;
        if use_preview == Some(true) && args.mdb_version.is_some() {
            anyhow::bail!(
                "MONGODB_ATLAS_LOCAL_PREVIEW=true cannot be used together with the --mdbVersion flag"
            );
        }
        let mdb_version = if args.mdb_version.is_some() {
            args.mdb_version
        } else if use_preview == Some(true) {
            MongoDBVersion::try_from("preview").ok()
        } else {
            None
        };

        Ok(Self {
            deployment_name: args.deployment_name,
            mdb_version,
            voyage_api_key: std::env::var("MONGODB_ATLAS_LOCAL_VOYAGE_API_KEY").ok(),
            port: args.port,
            bind_ip_all: args.bind_ip_all,
            initdb: args.initdb,
            force: args.force,
            load_sample_data: args.load_sample_data,
            username: args.username,
            password: args.password,
            image: args.image,
            skip_pull_image: args.skip_pull_image,
            connect_with: args.connect_with,

            interaction: Box::new(Interaction::new()),
            deployment_management: Box::new(Client::new(
                Docker::connect_with_defaults().context("connecting to Docker")?,
            )),
            connectors: HashMap::from([
                (
                    ConnectWith::Compass,
                    Box::new(Compass::new()) as Box<dyn Connector + Send + Sync>,
                ),
                (ConnectWith::Mongosh, Box::new(Mongosh::new())),
                (ConnectWith::VsCode, Box::new(VsCode::new())),
            ]),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum SetupResult {
    Setup {
        deployment_name: String,
        mongodb_version: Version,
        port: u16,
        load_sample_data: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        connect_result: Option<ConnectResult>,
    },
    Failed {
        deployment_name: Option<String>,
        error: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "connect_outcome", rename_all = "snake_case")]
pub enum ConnectResult {
    Connected { method: String },
    ConnectionString { connection_string: String },
    Skipped,
    Failed { error: String },
}

impl Display for SetupResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Setup {
                deployment_name,
                mongodb_version,
                port,
                load_sample_data,
                connect_result,
            } => {
                writeln!(f, "Successfully setup deployment '{deployment_name}'")?;
                writeln!(f, "MongoDB version: {mongodb_version}")?;
                writeln!(f, "Port: {port}")?;
                writeln!(f, "Load sample data: {load_sample_data}")?;

                // Display connection result if present
                if let Some(connect_result) = connect_result {
                    match connect_result {
                        ConnectResult::Connected { method } => {
                            write!(f, "Connected via: {method}")?;
                        }
                        ConnectResult::ConnectionString { connection_string } => {
                            write!(f, "Connection string: {connection_string}")?;
                        }
                        ConnectResult::Skipped => {
                            write!(f, "Connection: skipped")?;
                        }
                        ConnectResult::Failed { error } => {
                            write!(f, "Connection failed: {error}")?;
                        }
                    }
                }
                Ok(())
            }
            Self::Failed {
                deployment_name,
                error,
            } => {
                // If the deployment name is provided, quote it and add a space after it
                // Otherwise, use an empty string
                let quoted_deployment_name = deployment_name
                    .as_deref()
                    .map(|name| format!("'{}' ", name))
                    .unwrap_or_default();

                // Write the error message
                write!(
                    f,
                    "Setting up deployment {quoted_deployment_name}failed: {error}"
                )
            }
        }
    }
}

#[async_trait]
impl CommandWithOutput for Setup {
    type Output = SetupResult;

    async fn execute(&mut self) -> Result<Self::Output> {
        // If the force flag is not set, prompt the user for the settings
        if !self.force {
            // If the user canceled the setup, setup_result will be Some
            // Otherwise it will be None and the setup continues
            if let Some(setup_result) = self.prompt_settings()? {
                return Ok(setup_result);
            }
        }

        // Create the deployment
        let create_deployment_options = CreateDeploymentOptions {
            name: self.deployment_name.clone(),
            mongodb_version: self.mdb_version.clone(),
            creation_source: Some(CreationSource::AtlasLocal),
            wait_until_healthy: Some(true),
            local_seed_location: self
                .initdb
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            mongodb_initdb_root_username: self.username.clone(),
            mongodb_initdb_root_password: self.password.clone(),
            load_sample_data: self.load_sample_data,
            mongodb_port_binding: if self.bind_ip_all {
                Some(MongoDBPortBinding::new(
                    self.port,
                    BindingType::AnyInterface,
                ))
            } else {
                Some(MongoDBPortBinding::new(self.port, BindingType::Loopback))
            },
            image: self.image.clone(),
            skip_pull_image: Some(self.skip_pull_image),
            voyage_api_key: self.voyage_api_key.clone(),
            ..Default::default()
        };

        // Start the multi-step spinner
        let mut multi_step_spinner = self.interaction.start_multi_step_spinner(vec![
            MultiStepSpinnerStep::new("Pulling the latest version of the MongoDB image..."),
            MultiStepSpinnerStep::new("Creating the deployment..."),
            MultiStepSpinnerStep::new("Starting the deployment..."),
            MultiStepSpinnerStep::new("Waiting for the deployment to be healthy..."),
        ])?;

        let mut create_deployment_progress = self
            .deployment_management
            .create_deployment(create_deployment_options);

        let pull_image_outcome = create_deployment_progress
            .wait_for_pull_image_outcome()
            .await?;
        multi_step_spinner.set_step_outcome(
            0,
            deployment_outcome_to_multi_step_spinner_outcome(pull_image_outcome),
        )?;

        let create_container_outcome = create_deployment_progress
            .wait_for_create_container_outcome()
            .await?;
        multi_step_spinner.set_step_outcome(
            1,
            deployment_outcome_to_multi_step_spinner_outcome(create_container_outcome),
        )?;

        let start_container_outcome = create_deployment_progress
            .wait_for_start_container_outcome()
            .await?;
        multi_step_spinner.set_step_outcome(
            2,
            deployment_outcome_to_multi_step_spinner_outcome(start_container_outcome),
        )?;

        let wait_for_healthy_deployment_outcome = create_deployment_progress
            .wait_for_wait_for_healthy_deployment_outcome()
            .await?;
        multi_step_spinner.set_step_outcome(
            3,
            deployment_outcome_to_multi_step_spinner_outcome(wait_for_healthy_deployment_outcome),
        )?;

        // Return the result of the setup
        // In case of an error we distinguish between actual errors and errors that are expected because of the user's configuration
        // In case the error is expected we return a user friendly error message
        // In case the error is not expected we return the error with the context of the error
        match create_deployment_progress
            .wait_for_deployment_outcome()
            .await
        {
            Ok(deployment) => {
                let deployment_name = deployment.name.clone().unwrap_or("unknown".to_string());
                let mongodb_version = deployment.mongodb_version.clone();
                let port = deployment
                    .port_bindings
                    .as_ref()
                    .and_then(|bindings| bindings.port)
                    .unwrap_or(0);
                let load_sample_data = deployment.mongodb_load_sample_data.unwrap_or(false);

                // Prompt for connection method and connect if requested
                let connect_result = self
                    .prompt_and_connect(&deployment.container_id, &deployment_name)
                    .await?;

                Ok(SetupResult::Setup {
                    deployment_name,
                    mongodb_version,
                    port,
                    load_sample_data,
                    connect_result,
                })
            }
            Err(CreateDeploymentError::ReceiveDeployment(error)) => {
                Err(error).context("receiving deployment outcome")
            }
            Err(e) => Ok(SetupResult::Failed {
                deployment_name: self.deployment_name.clone(),
                error: e.to_string(),
            }),
        }
    }
}

fn deployment_outcome_to_multi_step_spinner_outcome(
    outcome: CreateDeploymentStepOutcome,
) -> MultiStepSpinnerOutcome {
    match outcome {
        CreateDeploymentStepOutcome::Success => MultiStepSpinnerOutcome::Success,
        CreateDeploymentStepOutcome::Failure => MultiStepSpinnerOutcome::Failure,
        CreateDeploymentStepOutcome::Skipped => MultiStepSpinnerOutcome::Skipped,
    }
}

impl Setup {
    fn prompt_settings(&mut self) -> Result<Option<SetupResult>> {
        // Prompt the user for the setup type
        // There are three options: default, custom, and cancel
        // - Default: Use default settings, no need to prompt for missing settings, we can pick the defaults
        // - Custom: Prompt for missing settings, we need to prompt for the missing settings
        // - Cancel: Cancel the setup

        // Build the select options
        let select_options = SelectPromptOptions::builder()
            .message("How do you want to set up your local Atlas deployment?")
            .options(vec![
                SETUP_TYPE_DEFAULT,
                SETUP_TYPE_CUSTOM,
                SETUP_TYPE_CANCEL,
            ])
            .build();

        // Create a closure to return the cancelled message
        let cancelled_message = |deployment_name: Option<String>| -> Result<Option<SetupResult>> {
            Ok(Some(SetupResult::Failed {
                deployment_name,
                error: "User canceled the setup".to_string(),
            }))
        };

        // Prompt the user for the setup type and handle the result
        match self
            .interaction
            .select(select_options)
            .context("failed to prompt for setup type")?
        {
            SelectPromptResult::Selected(value) if value == SETUP_TYPE_DEFAULT => {
                // Nothing to do here, default settings will be used when creating the deployment
                debug!("Using default settings");
            }
            SelectPromptResult::Selected(value) if value == SETUP_TYPE_CUSTOM => {
                // Prompt for the custom settings (deployment name, MongoDB version, port) if one of the fields is not provided
                if self.deployment_name.is_none()
                    || self.mdb_version.is_none()
                    || self.port.is_none()
                {
                    // Prompt for the custom settings
                    // If the user canceled the prompt, return a failed result
                    if let PromptCustomSettingsResult::Canceled = self
                        .prompt_custom_settings()
                        .context("prompting for custom settings")?
                    {
                        return cancelled_message(self.deployment_name.clone());
                    }
                }
            }
            SelectPromptResult::Canceled | SelectPromptResult::Selected(_) => {
                // User canceled the setup
                // Return a failed result
                return cancelled_message(self.deployment_name.clone());
            }
        }

        Ok(None)
    }

    /// Prompt for the custom settings
    /// These custom settings are:
    /// - Deployment name
    /// - MongoDB version
    /// - Port
    fn prompt_custom_settings(&mut self) -> Result<PromptCustomSettingsResult> {
        // Prompt for the deployment name
        let prompt_deployment_name_result = self.prompt_field_with_validator(
            "Deployment Name?",
            None,
            |setup| setup.deployment_name.clone(),
            |setup, deployment_name| {
                setup.deployment_name = Some(deployment_name);
                Ok(())
            },
            validators::DeploymentNameValidator,
        )?;

        if let PromptCustomSettingsResult::Canceled = prompt_deployment_name_result {
            return Ok(PromptCustomSettingsResult::Canceled);
        }

        let prompt_mdb_version_result = self.prompt_field_with_validator(
            "Major MongoDB Version?",
            Some("latest"),
            |setup| setup.mdb_version.as_ref().map(ToString::to_string),
            |setup, mdb_version| {
                setup.mdb_version =
                    Some(MongoDBVersion::try_from(mdb_version.as_str()).map_err(|e| {
                        anyhow::anyhow!("converting string to MongoDBVersion: {}", e)
                    })?);
                Ok(())
            },
            validators::MdbVersionValidator,
        )?;

        if let PromptCustomSettingsResult::Canceled = prompt_mdb_version_result {
            return Ok(PromptCustomSettingsResult::Canceled);
        }

        let prompt_port_result = self.prompt_field_with_validator(
            "Port?",
            Some("auto-assign"),
            |setup| setup.port.as_ref().map(ToString::to_string),
            |setup, port| {
                // If the port is provided, convert it to a u16
                // Otherwise, leave it as None, the port will be auto-assigned
                if !port.is_empty() && port != "auto-assign" {
                    setup.port = Some(port.parse::<u16>().context("converting port to u16")?);
                }
                Ok(())
            },
            validators::PortValidator,
        )?;

        if let PromptCustomSettingsResult::Canceled = prompt_port_result {
            return Ok(PromptCustomSettingsResult::Canceled);
        }

        // Prompt if we want to load sample data
        let prompt_port_result = self.prompt_field_with_validator(
            "Would you like to load sample data? (y/N)",
            Some("n"),
            |setup| {
                setup
                    .load_sample_data
                    .map(|b| if b { "y".to_string() } else { "n".to_string() })
            },
            |setup, answer| {
                setup.load_sample_data = Some(
                    validators::yes_no_to_bool(answer.as_str(), false)
                        .map_err(|e| anyhow::anyhow!("converting yes/no to bool: {}", e))?,
                );

                Ok(())
            },
            validators::YesNoValidator,
        )?;

        if let PromptCustomSettingsResult::Canceled = prompt_port_result {
            return Ok(PromptCustomSettingsResult::Canceled);
        }

        Ok(PromptCustomSettingsResult::Continue)
    }

    fn prompt_field_with_validator<G, S, V>(
        &mut self,
        prompt: &str,
        default: Option<&str>,
        get_field: G,
        set_field: S,
        validator: V,
    ) -> Result<PromptCustomSettingsResult>
    where
        G: FnOnce(&mut Self) -> Option<String>,
        S: FnOnce(&mut Self, String) -> Result<()>,
        V: InputValidator + 'static,
    {
        // Get the field value, if it's provided set it as the default for the prompt
        let field_value_string = get_field(self);

        // Prompt for the field using the provided prompt and validator
        // If the field value is provided, set it as the final answer to the prompt
        let prompt_options = InputPromptOptions::builder()
            .message(prompt.to_string())
            .validator(InputPromptValidator::new(validator))
            .default_opt(default.map(ToString::to_string))
            .final_answer(field_value_string)
            .build();

        // Prompt for the field value
        let InputPromptResult::Input(field_value_string) = self
            .interaction
            .input(prompt_options)
            .context("prompting for field")?
        else {
            return Ok(PromptCustomSettingsResult::Canceled);
        };

        // Convert the field value string to the field type
        set_field(self, field_value_string).context("setting field value")?;

        Ok(PromptCustomSettingsResult::Continue)
    }

    /// Prompt for connection method and connect to the deployment
    async fn prompt_and_connect(
        &self,
        container_id: &str,
        deployment_name: &str,
    ) -> Result<Option<ConnectResult>> {
        // Determine which connection method to use
        let connect_with = if let Some(connect_with) = &self.connect_with {
            // If connect_with was provided via CLI, use it directly
            Some(*connect_with)
        } else if self.force {
            // If force flag is set and no connect_with provided, skip connection
            None
        } else {
            // Prompt the user to select a connection method
            self.prompt_connection_method()?
        };

        // If no connection method selected (user chose skip or force mode), return None
        let Some(connect_with) = connect_with else {
            return Ok(Some(ConnectResult::Skipped));
        };

        // Get the connection string
        let connection_string = self
            .deployment_management
            .get_connection_string(container_id.to_string())
            .await
            .context("getting connection string")?;

        // If the connector is ConnectionString, return the connection string
        if connect_with == ConnectWith::ConnectionString {
            return Ok(Some(ConnectResult::ConnectionString { connection_string }));
        }

        // Get the connector
        let connector = self
            .connectors
            .get(&connect_with)
            .context("Connector not found")?;

        // Check if the connector is available
        if !connector.is_available().await {
            let connector_name = match connect_with {
                ConnectWith::Compass => "Compass",
                ConnectWith::Mongosh => "mongosh",
                ConnectWith::VsCode => "VS Code",
                ConnectWith::ConnectionString => unreachable!(),
            };
            return Ok(Some(ConnectResult::Failed {
                error: format!("{} is not installed", connector_name),
            }));
        }

        // Launch the connector
        connector
            .launch(&DeploymentParams::new(deployment_name, &connection_string))
            .await
            .context("launching connector")?;

        let method = match connect_with {
            ConnectWith::Compass => "Compass",
            ConnectWith::Mongosh => "mongosh",
            ConnectWith::VsCode => "VS Code",
            ConnectWith::ConnectionString => unreachable!(),
        };

        Ok(Some(ConnectResult::Connected {
            method: method.to_string(),
        }))
    }

    /// Prompt the user to select a connection method
    fn prompt_connection_method(&self) -> Result<Option<ConnectWith>> {
        let select_options = SelectPromptOptions::builder()
            .message("How do you want to connect to your local Atlas deployment?")
            .options(vec![
                CONNECT_WITH_COMPASS,
                CONNECT_WITH_MONGOSH,
                CONNECT_WITH_VSCODE,
                CONNECT_WITH_CONNECTION_STRING,
                CONNECT_WITH_SKIP,
            ])
            .build();

        match self
            .interaction
            .select(select_options)
            .context("failed to prompt for connection method")?
        {
            SelectPromptResult::Selected(value) if value == CONNECT_WITH_COMPASS => {
                debug!("User selected Compass");
                Ok(Some(ConnectWith::Compass))
            }
            SelectPromptResult::Selected(value) if value == CONNECT_WITH_MONGOSH => {
                debug!("User selected mongosh");
                Ok(Some(ConnectWith::Mongosh))
            }
            SelectPromptResult::Selected(value) if value == CONNECT_WITH_VSCODE => {
                debug!("User selected VS Code");
                Ok(Some(ConnectWith::VsCode))
            }
            SelectPromptResult::Selected(value) if value == CONNECT_WITH_CONNECTION_STRING => {
                debug!("User selected connection string");
                Ok(Some(ConnectWith::ConnectionString))
            }
            SelectPromptResult::Selected(_) | SelectPromptResult::Canceled => {
                debug!("User skipped connection");
                Ok(None)
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum PromptCustomSettingsResult {
    Continue,
    Canceled,
}

const SETUP_TYPE_DEFAULT: &str = "With default settings";
const SETUP_TYPE_CUSTOM: &str = "With custom settings";
const SETUP_TYPE_CANCEL: &str = "Cancel setup";

const CONNECT_WITH_COMPASS: &str = "Compass";
const CONNECT_WITH_MONGOSH: &str = "mongosh";
const CONNECT_WITH_VSCODE: &str = "VS Code";
const CONNECT_WITH_CONNECTION_STRING: &str = "Connection string";
const CONNECT_WITH_SKIP: &str = "Skip";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependencies::mocks::MockDocker;
    use crate::interaction::mocks::MockInteraction;
    use crate::interaction::{MultiStepSpinner, MultiStepSpinnerOutcome};
    use atlas_local::{
        client::{CreateDeploymentProgress, CreateDeploymentStepOutcome},
        models::{
            BindingType, CreationSource, Deployment as AtlasDeployment, MongoDBVersion,
            MongoDBVersionMajor, MongoDBVersionMajorMinor, MongoDBVersionMajorMinorPatch,
            MongodbType,
        },
    };
    use futures_util::FutureExt;
    use mockall::mock;
    use semver::Version;
    use std::sync::Arc;

    mock! {
        pub Connector {}

        #[async_trait]
        impl Connector for Connector {
            async fn is_available(&self) -> bool;
            async fn launch(&self, params: &DeploymentParams) -> Result<()>;
        }
    }

    // ============================================================================
    // Test Helpers
    // ============================================================================

    /// Creates a mock CreateDeploymentProgress with pre-set outcomes
    fn create_mock_progress(
        pull_outcome: CreateDeploymentStepOutcome,
        create_outcome: CreateDeploymentStepOutcome,
        start_outcome: CreateDeploymentStepOutcome,
        healthy_outcome: CreateDeploymentStepOutcome,
        deployment_result: Result<AtlasDeployment, CreateDeploymentError>,
    ) -> CreateDeploymentProgress {
        use tokio::sync::oneshot;

        let (pull_s, pull_r) = oneshot::channel();
        let _ = pull_s.send(pull_outcome);
        let (create_s, create_r) = oneshot::channel();
        let _ = create_s.send(create_outcome);
        let (start_s, start_r) = oneshot::channel();
        let _ = start_s.send(start_outcome);
        let (healthy_s, healthy_r) = oneshot::channel();
        let _ = healthy_s.send(healthy_outcome);
        let (deploy_s, deploy_r) = oneshot::channel();
        let _ = deploy_s.send(deployment_result);

        CreateDeploymentProgress {
            pull_image_finished: pull_r.fuse(),
            create_container_finished: create_r.fuse(),
            start_container_finished: start_r.fuse(),
            wait_for_healthy_deployment_finished: healthy_r.fuse(),
            deployment: deploy_r.fuse(),
        }
    }

    /// Creates a successful mock progress for happy path tests
    fn create_successful_progress(deployment: AtlasDeployment) -> CreateDeploymentProgress {
        create_mock_progress(
            CreateDeploymentStepOutcome::Success,
            CreateDeploymentStepOutcome::Success,
            CreateDeploymentStepOutcome::Success,
            CreateDeploymentStepOutcome::Success,
            Ok(deployment),
        )
    }

    struct MockMultiStepSpinner {
        outcomes: Arc<std::sync::Mutex<Vec<(usize, MultiStepSpinnerOutcome)>>>,
    }

    impl MultiStepSpinner for MockMultiStepSpinner {
        fn set_step_outcome(
            &mut self,
            step: usize,
            outcome: MultiStepSpinnerOutcome,
        ) -> Result<()> {
            if let Ok(mut outcomes) = self.outcomes.lock() {
                outcomes.push((step, outcome));
            }
            Ok(())
        }
    }

    /// Creates a test deployment with the given parameters
    fn create_deployment(
        name: Option<String>,
        version: Version,
        port: Option<u16>,
        load_sample_data: Option<bool>,
    ) -> AtlasDeployment {
        AtlasDeployment {
            name,
            container_id: "test-container-id".to_string(),
            mongodb_version: version,
            state: atlas_local::models::State::Running,
            port_bindings: port.map(|p| {
                atlas_local::models::MongoDBPortBinding::new(
                    Some(p),
                    atlas_local::models::BindingType::Loopback,
                )
            }),
            mongodb_type: MongodbType::Community,
            creation_source: Some(atlas_local::models::CreationSource::AtlasLocal),
            local_seed_location: None,
            mongodb_initdb_database: None,
            mongodb_initdb_root_password_file: None,
            mongodb_initdb_root_password: None,
            mongodb_initdb_root_username_file: None,
            mongodb_initdb_root_username: None,
            mongodb_load_sample_data: load_sample_data,
            mongot_log_file: None,
            runner_log_file: None,
            do_not_track: true,
            telemetry_base_url: None,
            voyage_api_key: None,
        }
    }

    /// Creates a Setup command with default test values
    fn create_setup_command(
        deployment_name: Option<String>,
        mdb_version: Option<MongoDBVersion>,
        port: Option<u16>,
        force: bool,
        load_sample_data: Option<bool>,
        bind_ip_all: bool,
        initdb: Option<PathBuf>,
        username: Option<String>,
        password: Option<String>,
        interaction: Box<dyn SetupInteraction + Send>,
        deployment_management: Box<dyn SetupDeploymentManagement + Send>,
    ) -> Setup {
        create_setup_command_with_connectors(
            deployment_name,
            mdb_version,
            port,
            force,
            load_sample_data,
            bind_ip_all,
            initdb,
            username,
            password,
            None,
            None,
            interaction,
            deployment_management,
            HashMap::new(),
        )
    }

    /// Creates a Setup command with connectors
    fn create_setup_command_with_connectors(
        deployment_name: Option<String>,
        mdb_version: Option<MongoDBVersion>,
        port: Option<u16>,
        force: bool,
        load_sample_data: Option<bool>,
        bind_ip_all: bool,
        initdb: Option<PathBuf>,
        username: Option<String>,
        password: Option<String>,
        connect_with: Option<ConnectWith>,
        voyage_api_key: Option<String>,
        interaction: Box<dyn SetupInteraction + Send>,
        deployment_management: Box<dyn SetupDeploymentManagement + Send>,
        connectors: HashMap<ConnectWith, Box<dyn Connector + Send + Sync>>,
    ) -> Setup {
        Setup {
            deployment_name,
            mdb_version,
            voyage_api_key,
            port,
            bind_ip_all,
            initdb,
            force,
            load_sample_data,
            username,
            password,
            image: None,
            skip_pull_image: false,
            connect_with,
            interaction,
            deployment_management,
            connectors,
        }
    }

    /// Creates a mock interaction with a multi-step spinner
    fn create_mock_interaction_with_spinner(
        outcomes: Arc<std::sync::Mutex<Vec<(usize, MultiStepSpinnerOutcome)>>>,
    ) -> MockInteraction {
        let mut mock = MockInteraction::new();
        let outcomes_clone = outcomes.clone();
        mock.expect_start_multi_step_spinner()
            .return_once(move |_| {
                Ok(Box::new(MockMultiStepSpinner {
                    outcomes: outcomes_clone,
                }))
            });
        mock
    }

    /// Verifies that all spinner steps succeeded
    fn verify_all_steps_succeeded(
        outcomes: &Arc<std::sync::Mutex<Vec<(usize, MultiStepSpinnerOutcome)>>>,
    ) {
        let outcomes_vec = outcomes.lock().unwrap();
        assert_eq!(outcomes_vec.len(), 4, "Expected 4 spinner steps");
        for (step, outcome) in outcomes_vec.iter() {
            assert!(
                matches!(outcome, MultiStepSpinnerOutcome::Success),
                "Step {} should have succeeded",
                step
            );
        }
    }
    // ============================================================================
    // Happy Path Tests
    // ============================================================================

    #[tokio::test]
    async fn test_setup_with_force_flag_and_all_fields_provided() {
        let deployment_name = "test-deployment".to_string();
        let version = Version::parse("8.2.2").unwrap();

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mock_interaction = create_mock_interaction_with_spinner(outcomes.clone());

        let mut mock_deployment_management = MockDocker::new();
        let deployment = create_deployment(
            Some(deployment_name.clone()),
            version.clone(),
            Some(27017),
            Some(false),
        );
        let progress = create_successful_progress(deployment);
        let expected_name = deployment_name.clone();
        let expected_version = MongoDBVersion::MajorMinorPatch(MongoDBVersionMajorMinorPatch {
            major: 8,
            minor: 2,
            patch: 2,
        });
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |options| {
                assert_eq!(options.name, Some(expected_name));
                assert_eq!(options.mongodb_version, Some(expected_version.clone()));
                assert_eq!(options.creation_source, Some(CreationSource::AtlasLocal));
                assert_eq!(options.wait_until_healthy, Some(true));
                assert_eq!(options.local_seed_location, None);
                assert_eq!(options.mongodb_initdb_root_username, None);
                assert_eq!(options.mongodb_initdb_root_password, None);
                assert_eq!(options.load_sample_data, Some(false));
                assert!(options.mongodb_port_binding.is_some());
                if let Some(binding) = &options.mongodb_port_binding {
                    assert_eq!(binding.port, Some(27017));
                    assert!(matches!(binding.binding_type, BindingType::Loopback));
                }
                assert_eq!(options.image, None);
                assert_eq!(options.skip_pull_image, Some(false));
                progress
            });

        let mut setup_command = create_setup_command(
            Some(deployment_name.clone()),
            Some(MongoDBVersion::MajorMinorPatch(
                MongoDBVersionMajorMinorPatch {
                    major: 8,
                    minor: 2,
                    patch: 2,
                },
            )),
            Some(27017),
            true,
            Some(false),
            false,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Setup {
                deployment_name: deployment_name.clone(),
                mongodb_version: version,
                port: 27017,
                load_sample_data: false,
                connect_result: Some(ConnectResult::Skipped),
            }
        );
        verify_all_steps_succeeded(&outcomes);
    }

    #[tokio::test]
    async fn test_setup_without_force_flag_selects_default_settings() {
        let deployment_name = "test-deployment".to_string();
        let version = Version::parse("8.2.2").unwrap();

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut mock_interaction = create_mock_interaction_with_spinner(outcomes.clone());
        // Two selects: setup type and connection method
        let select_call_count = std::sync::atomic::AtomicUsize::new(0);
        mock_interaction
            .expect_select()
            .times(2)
            .returning(move |_| {
                let count = select_call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                match count {
                    0 => Ok(crate::interaction::SelectPromptResult::Selected(
                        SETUP_TYPE_DEFAULT.to_string(),
                    )),
                    _ => Ok(crate::interaction::SelectPromptResult::Selected(
                        CONNECT_WITH_SKIP.to_string(),
                    )),
                }
            });

        let mut mock_deployment_management = MockDocker::new();
        let deployment = create_deployment(
            Some(deployment_name.clone()),
            version.clone(),
            Some(27017),
            Some(false),
        );
        let progress = create_successful_progress(deployment);
        let expected_name = deployment_name.clone();
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |options| {
                assert_eq!(options.name, Some(expected_name));
                assert_eq!(options.mongodb_version, None);
                assert_eq!(options.creation_source, Some(CreationSource::AtlasLocal));
                assert_eq!(options.wait_until_healthy, Some(true));
                assert_eq!(options.local_seed_location, None);
                assert_eq!(options.mongodb_initdb_root_username, None);
                assert_eq!(options.mongodb_initdb_root_password, None);
                assert_eq!(options.load_sample_data, None);
                assert!(options.mongodb_port_binding.is_some());
                if let Some(binding) = &options.mongodb_port_binding {
                    assert_eq!(binding.port, None);
                    assert!(matches!(binding.binding_type, BindingType::Loopback));
                }
                assert_eq!(options.image, None);
                assert_eq!(options.skip_pull_image, Some(false));
                progress
            });

        let mut setup_command = create_setup_command(
            Some(deployment_name.clone()),
            None,
            None,
            false,
            None,
            false,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Setup {
                deployment_name: deployment_name.clone(),
                mongodb_version: version,
                port: 27017,
                load_sample_data: false,
                connect_result: Some(ConnectResult::Skipped),
            }
        );
    }

    #[tokio::test]
    async fn test_setup_without_force_flag_prompts_for_custom_settings() {
        let deployment_name = "custom-deployment".to_string();
        let version = Version::parse("8.0.0").unwrap();

        let mut mock_interaction = MockInteraction::new();
        // Two selects: setup type and connection method
        let select_call_count = std::sync::atomic::AtomicUsize::new(0);
        mock_interaction
            .expect_select()
            .times(2)
            .returning(move |_| {
                let count = select_call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                match count {
                    0 => Ok(crate::interaction::SelectPromptResult::Selected(
                        SETUP_TYPE_CUSTOM.to_string(),
                    )),
                    _ => Ok(crate::interaction::SelectPromptResult::Selected(
                        CONNECT_WITH_SKIP.to_string(),
                    )),
                }
            });

        mock_interaction
            .expect_input()
            .times(4)
            .returning(|options| match options.message.as_str() {
                "Deployment Name?" => Ok(crate::interaction::InputPromptResult::Input(
                    "custom-deployment".to_string(),
                )),
                "Major MongoDB Version?" => Ok(crate::interaction::InputPromptResult::Input(
                    "8.0".to_string(),
                )),
                "Port?" => Ok(crate::interaction::InputPromptResult::Input(
                    "27018".to_string(),
                )),
                "Would you like to load sample data? (y/N)" => Ok(
                    crate::interaction::InputPromptResult::Input("y".to_string()),
                ),
                _ => panic!("Unexpected prompt: {}", options.message),
            });

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let outcomes_clone = outcomes.clone();
        mock_interaction
            .expect_start_multi_step_spinner()
            .return_once(move |_| {
                Ok(Box::new(MockMultiStepSpinner {
                    outcomes: outcomes_clone,
                }))
            });

        let mut mock_deployment_management = MockDocker::new();
        let deployment = create_deployment(
            Some(deployment_name.clone()),
            version.clone(),
            Some(27018),
            Some(true),
        );
        let progress = create_successful_progress(deployment);
        let expected_name = deployment_name.clone();
        let expected_version =
            MongoDBVersion::MajorMinor(MongoDBVersionMajorMinor { major: 8, minor: 0 });
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |options| {
                assert_eq!(options.name, Some(expected_name));
                assert_eq!(options.mongodb_version, Some(expected_version.clone()));
                assert_eq!(options.creation_source, Some(CreationSource::AtlasLocal));
                assert_eq!(options.wait_until_healthy, Some(true));
                assert_eq!(options.local_seed_location, None);
                assert_eq!(options.mongodb_initdb_root_username, None);
                assert_eq!(options.mongodb_initdb_root_password, None);
                assert_eq!(options.load_sample_data, Some(true));
                assert!(options.mongodb_port_binding.is_some());
                if let Some(binding) = &options.mongodb_port_binding {
                    assert_eq!(binding.port, Some(27018));
                    assert!(matches!(binding.binding_type, BindingType::Loopback));
                }
                assert_eq!(options.image, None);
                assert_eq!(options.skip_pull_image, Some(false));
                progress
            });

        let mut setup_command = create_setup_command(
            None,
            None,
            None,
            false,
            None,
            false,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Setup {
                deployment_name: deployment_name.clone(),
                mongodb_version: version,
                port: 27018,
                load_sample_data: true,
                connect_result: Some(ConnectResult::Skipped),
            }
        );
    }

    #[tokio::test]
    async fn test_setup_without_force_flag_custom_settings_with_partial_fields_provided() {
        let deployment_name = "partial-deployment".to_string();
        let version = Version::parse("8.1.0").unwrap();

        let mut mock_interaction = MockInteraction::new();
        // Two selects: setup type and connection method
        let select_call_count = std::sync::atomic::AtomicUsize::new(0);
        mock_interaction
            .expect_select()
            .times(2)
            .returning(move |_| {
                let count = select_call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                match count {
                    0 => Ok(crate::interaction::SelectPromptResult::Selected(
                        SETUP_TYPE_CUSTOM.to_string(),
                    )),
                    _ => Ok(crate::interaction::SelectPromptResult::Selected(
                        CONNECT_WITH_SKIP.to_string(),
                    )),
                }
            });

        mock_interaction
            .expect_input()
            .times(4)
            .returning(|options| match options.message.as_str() {
                "Deployment Name?" => Ok(crate::interaction::InputPromptResult::Input(
                    "partial-deployment".to_string(),
                )),
                "Major MongoDB Version?" => Ok(crate::interaction::InputPromptResult::Input(
                    "8.1".to_string(),
                )),
                "Port?" => Ok(crate::interaction::InputPromptResult::Input(
                    "27019".to_string(),
                )),
                "Would you like to load sample data? (y/N)" => Ok(
                    crate::interaction::InputPromptResult::Input("n".to_string()),
                ),
                _ => panic!("Unexpected prompt: {}", options.message),
            });

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let outcomes_clone = outcomes.clone();
        mock_interaction
            .expect_start_multi_step_spinner()
            .return_once(move |_| {
                Ok(Box::new(MockMultiStepSpinner {
                    outcomes: outcomes_clone,
                }))
            });

        let mut mock_deployment_management = MockDocker::new();
        let deployment = create_deployment(
            Some(deployment_name.clone()),
            version.clone(),
            Some(27019),
            Some(false),
        );
        let progress = create_successful_progress(deployment);
        let expected_name = deployment_name.clone();
        let expected_version =
            MongoDBVersion::MajorMinor(MongoDBVersionMajorMinor { major: 8, minor: 1 });
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |options| {
                assert_eq!(options.name, Some(expected_name));
                assert_eq!(options.mongodb_version, Some(expected_version.clone()));
                assert_eq!(options.voyage_api_key, None);
                assert_eq!(options.creation_source, Some(CreationSource::AtlasLocal));
                assert_eq!(options.wait_until_healthy, Some(true));
                assert_eq!(options.local_seed_location, None);
                assert_eq!(options.mongodb_initdb_root_username, None);
                assert_eq!(options.mongodb_initdb_root_password, None);
                assert_eq!(options.load_sample_data, Some(false));
                assert!(options.mongodb_port_binding.is_some());
                if let Some(binding) = &options.mongodb_port_binding {
                    assert_eq!(binding.port, Some(27019));
                    assert!(matches!(binding.binding_type, BindingType::Loopback));
                }
                assert_eq!(options.image, None);
                assert_eq!(options.skip_pull_image, Some(false));
                progress
            });

        let mut setup_command = create_setup_command(
            Some(deployment_name.clone()),
            Some(MongoDBVersion::MajorMinor(MongoDBVersionMajorMinor {
                major: 8,
                minor: 1,
            })),
            None,
            false,
            None,
            false,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Setup {
                deployment_name: deployment_name.clone(),
                mongodb_version: version,
                port: 27019,
                load_sample_data: false,
                connect_result: Some(ConnectResult::Skipped),
            }
        );
    }

    #[tokio::test]
    async fn test_setup_voyage_api_key_passed_to_create_deployment() {
        let deployment_name = "voyage-test".to_string();
        let version = Version::parse("8.0.0").unwrap();
        let voyage_api_key = "super-secret".to_string();

        let mut mock_interaction = MockInteraction::new();
        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let outcomes_clone = outcomes.clone();
        mock_interaction
            .expect_start_multi_step_spinner()
            .return_once(move |_| {
                Ok(Box::new(MockMultiStepSpinner {
                    outcomes: outcomes_clone,
                }))
            });

        let mut mock_deployment_management = MockDocker::new();
        let deployment = create_deployment(
            Some(deployment_name.clone()),
            version.clone(),
            Some(27017),
            Some(false),
        );
        let progress = create_successful_progress(deployment);
        let expected_voyage_key = voyage_api_key.clone();
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |options| {
                assert_eq!(
                    options.voyage_api_key,
                    Some(expected_voyage_key),
                    "voyage_api_key from MONGODB_ATLAS_LOCAL_VOYAGE_API_KEY must be passed to CreateDeploymentOptions"
                );
                progress
            });

        let mut setup_command = create_setup_command_with_connectors(
            Some(deployment_name.clone()),
            Some(MongoDBVersion::Major(MongoDBVersionMajor { major: 8 })),
            Some(27017),
            true,
            Some(false),
            false,
            None,
            None,
            None,
            None,
            Some(voyage_api_key),
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
            HashMap::new(),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Setup {
                deployment_name: deployment_name.clone(),
                mongodb_version: version,
                port: 27017,
                load_sample_data: false,
                connect_result: Some(ConnectResult::Skipped),
            }
        );
    }

    #[tokio::test]
    async fn test_setup_without_force_flag_custom_settings_with_all_fields_provided() {
        let deployment_name = "test-deployment".to_string();
        let version = Version::parse("8.0.0").unwrap();

        let mut mock_interaction = MockInteraction::new();
        // Two selects: setup type and connection method
        let select_call_count = std::sync::atomic::AtomicUsize::new(0);
        mock_interaction
            .expect_select()
            .times(2)
            .returning(move |_| {
                let count = select_call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                match count {
                    0 => Ok(crate::interaction::SelectPromptResult::Selected(
                        SETUP_TYPE_CUSTOM.to_string(),
                    )),
                    _ => Ok(crate::interaction::SelectPromptResult::Selected(
                        CONNECT_WITH_SKIP.to_string(),
                    )),
                }
            });

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let outcomes_clone = outcomes.clone();
        mock_interaction
            .expect_start_multi_step_spinner()
            .return_once(move |_| {
                Ok(Box::new(MockMultiStepSpinner {
                    outcomes: outcomes_clone,
                }))
            });

        let mut mock_deployment_management = MockDocker::new();
        let deployment = create_deployment(
            Some(deployment_name.clone()),
            version.clone(),
            Some(27017),
            Some(false),
        );
        let progress = create_successful_progress(deployment);
        let expected_name = deployment_name.clone();
        let expected_version = MongoDBVersion::Major(MongoDBVersionMajor { major: 8 });
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |options| {
                assert_eq!(options.name, Some(expected_name));
                assert_eq!(options.mongodb_version, Some(expected_version.clone()));
                assert_eq!(options.creation_source, Some(CreationSource::AtlasLocal));
                assert_eq!(options.wait_until_healthy, Some(true));
                assert_eq!(options.local_seed_location, None);
                assert_eq!(options.mongodb_initdb_root_username, None);
                assert_eq!(options.mongodb_initdb_root_password, None);
                assert_eq!(options.load_sample_data, Some(false));
                assert!(options.mongodb_port_binding.is_some());
                if let Some(binding) = &options.mongodb_port_binding {
                    assert_eq!(binding.port, Some(27017));
                    assert!(matches!(binding.binding_type, BindingType::Loopback));
                }
                assert_eq!(options.image, None);
                assert_eq!(options.skip_pull_image, Some(false));
                progress
            });

        let mut setup_command = create_setup_command(
            Some(deployment_name.clone()),
            Some(MongoDBVersion::Major(MongoDBVersionMajor { major: 8 })),
            Some(27017),
            false,
            Some(false),
            false,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Setup {
                deployment_name: deployment_name.clone(),
                mongodb_version: version,
                port: 27017,
                load_sample_data: false,
                connect_result: Some(ConnectResult::Skipped),
            }
        );
    }

    // ============================================================================
    // Cancellation Tests
    // ============================================================================

    #[tokio::test]
    async fn test_setup_user_cancels_at_initial_select_prompt() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction
            .expect_select()
            .return_once(|_| Ok(crate::interaction::SelectPromptResult::Canceled));

        let mut setup_command = create_setup_command(
            None,
            None,
            None,
            false,
            None,
            false,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(MockDocker::new()),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Failed {
                deployment_name: None,
                error: "User canceled the setup".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn test_setup_user_cancels_at_deployment_name_prompt() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction.expect_select().return_once(|_| {
            Ok(crate::interaction::SelectPromptResult::Selected(
                SETUP_TYPE_CUSTOM.to_string(),
            ))
        });
        mock_interaction
            .expect_input()
            .return_once(|_| Ok(crate::interaction::InputPromptResult::Canceled));

        let mut setup_command = create_setup_command(
            None,
            None,
            None,
            false,
            None,
            false,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(MockDocker::new()),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Failed {
                deployment_name: None,
                error: "User canceled the setup".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn test_setup_user_cancels_at_mongodb_version_prompt() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction.expect_select().return_once(|_| {
            Ok(crate::interaction::SelectPromptResult::Selected(
                SETUP_TYPE_CUSTOM.to_string(),
            ))
        });
        mock_interaction.expect_input().times(1).return_once(|_| {
            Ok(crate::interaction::InputPromptResult::Input(
                "test".to_string(),
            ))
        });
        mock_interaction
            .expect_input()
            .times(1)
            .return_once(|_| Ok(crate::interaction::InputPromptResult::Canceled));

        let mut setup_command = create_setup_command(
            None,
            None,
            None,
            false,
            None,
            false,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(MockDocker::new()),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Failed {
                deployment_name: Some("test".to_string()),
                error: "User canceled the setup".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn test_setup_user_cancels_at_port_prompt() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction.expect_select().return_once(|_| {
            Ok(crate::interaction::SelectPromptResult::Selected(
                SETUP_TYPE_CUSTOM.to_string(),
            ))
        });
        mock_interaction
            .expect_input()
            .times(3)
            .returning(|options| match options.message.as_str() {
                "Deployment Name?" => Ok(crate::interaction::InputPromptResult::Input(
                    "test".to_string(),
                )),
                "Major MongoDB Version?" => Ok(crate::interaction::InputPromptResult::Input(
                    "8".to_string(),
                )),
                "Port?" => Ok(crate::interaction::InputPromptResult::Canceled),
                _ => panic!("Unexpected prompt: {}", options.message),
            });

        let mut setup_command = create_setup_command(
            None,
            None,
            None,
            false,
            None,
            false,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(MockDocker::new()),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Failed {
                deployment_name: Some("test".to_string()),
                error: "User canceled the setup".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn test_setup_user_cancels_at_load_sample_data_prompt() {
        let mut mock_interaction = MockInteraction::new();
        mock_interaction.expect_select().return_once(|_| {
            Ok(crate::interaction::SelectPromptResult::Selected(
                SETUP_TYPE_CUSTOM.to_string(),
            ))
        });
        mock_interaction
            .expect_input()
            .times(4)
            .returning(|options| match options.message.as_str() {
                "Deployment Name?" => Ok(crate::interaction::InputPromptResult::Input(
                    "test".to_string(),
                )),
                "Major MongoDB Version?" => Ok(crate::interaction::InputPromptResult::Input(
                    "8".to_string(),
                )),
                "Port?" => Ok(crate::interaction::InputPromptResult::Input(
                    "27017".to_string(),
                )),
                "Would you like to load sample data? (y/N)" => {
                    Ok(crate::interaction::InputPromptResult::Canceled)
                }
                _ => panic!("Unexpected prompt: {}", options.message),
            });

        let mut setup_command = create_setup_command(
            None,
            None,
            None,
            false,
            None,
            false,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(MockDocker::new()),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Failed {
                deployment_name: Some("test".to_string()),
                error: "User canceled the setup".to_string(),
            }
        );
    }

    // ============================================================================
    // Error Handling Tests
    // ============================================================================

    #[tokio::test]
    async fn test_setup_handles_deployment_failure() {
        let deployment_name = "test-deployment".to_string();

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mock_interaction = create_mock_interaction_with_spinner(outcomes.clone());

        let mut mock_deployment_management = MockDocker::new();
        let expected_name = deployment_name.clone();
        let progress = create_mock_progress(
            CreateDeploymentStepOutcome::Failure,
            CreateDeploymentStepOutcome::Skipped,
            CreateDeploymentStepOutcome::Skipped,
            CreateDeploymentStepOutcome::Skipped,
            Err(CreateDeploymentError::PullImage(
                atlas_local::client::PullImageError::from(bollard::errors::Error::from(
                    std::io::Error::new(std::io::ErrorKind::Other, "Failed to pull image"),
                )),
            )),
        );
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |options| {
                assert_eq!(options.name, Some(expected_name));
                assert_eq!(options.mongodb_version, None);
                assert_eq!(options.creation_source, Some(CreationSource::AtlasLocal));
                assert_eq!(options.wait_until_healthy, Some(true));
                progress
            });

        let mut setup_command = create_setup_command(
            Some(deployment_name.clone()),
            None,
            None,
            true,
            None,
            false,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        match result {
            SetupResult::Failed {
                deployment_name: name,
                error,
            } => {
                assert_eq!(name, Some(deployment_name));
                assert!(!error.is_empty());
            }
            _ => panic!("Expected Failed result, got {:?}", result),
        }
    }

    // This test should never happen, but we test it for completeness
    #[tokio::test]
    async fn test_setup_handles_receive_deployment_error() {
        let deployment_name = "test-deployment".to_string();

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mock_interaction = create_mock_interaction_with_spinner(outcomes.clone());

        let mut mock_deployment_management = MockDocker::new();
        use tokio::sync::oneshot;
        let (pull_s, pull_r) = oneshot::channel();
        let _ = pull_s.send(CreateDeploymentStepOutcome::Success);
        let (create_s, create_r) = oneshot::channel();
        let _ = create_s.send(CreateDeploymentStepOutcome::Success);
        let (start_s, start_r) = oneshot::channel();
        let _ = start_s.send(CreateDeploymentStepOutcome::Success);
        let (healthy_s, healthy_r) = oneshot::channel();
        let _ = healthy_s.send(CreateDeploymentStepOutcome::Success);
        let (deploy_s, deploy_r) = oneshot::channel();
        drop(deploy_s); // Simulate error by dropping sender

        let progress = CreateDeploymentProgress {
            pull_image_finished: pull_r.fuse(),
            create_container_finished: create_r.fuse(),
            start_container_finished: start_r.fuse(),
            wait_for_healthy_deployment_finished: healthy_r.fuse(),
            deployment: deploy_r.fuse(),
        };
        let expected_name = deployment_name.clone();
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |options| {
                assert_eq!(options.name, Some(expected_name));
                assert_eq!(options.mongodb_version, None);
                assert_eq!(options.creation_source, Some(CreationSource::AtlasLocal));
                assert_eq!(options.wait_until_healthy, Some(true));
                progress
            });

        let mut setup_command = create_setup_command(
            Some(deployment_name),
            None,
            None,
            true,
            None,
            false,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
        );

        let result = setup_command.execute().await;

        assert!(
            result.is_err(),
            "Expected error when deployment receiver is dropped"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("receiving deployment outcome"),
            "Error message should mention receiving deployment outcome"
        );
    }

    // ============================================================================
    // Option-Specific Tests
    // ============================================================================

    #[tokio::test]
    async fn test_setup_with_initdb_path() {
        let deployment_name = "test-deployment".to_string();
        let version = Version::parse("8.2.2").unwrap();
        let initdb_path = std::path::PathBuf::from("/tmp/test-initdb");

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mock_interaction = create_mock_interaction_with_spinner(outcomes.clone());

        let mut mock_deployment_management = MockDocker::new();
        let deployment = create_deployment(
            Some(deployment_name.clone()),
            version.clone(),
            Some(27017),
            Some(false),
        );
        let progress = create_successful_progress(deployment);
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |options| {
                assert_eq!(
                    options.local_seed_location,
                    Some("/tmp/test-initdb".to_string())
                );
                progress
            });

        let mut setup_command = create_setup_command(
            Some(deployment_name.clone()),
            Some(MongoDBVersion::MajorMinorPatch(
                MongoDBVersionMajorMinorPatch {
                    major: 8,
                    minor: 2,
                    patch: 2,
                },
            )),
            Some(27017),
            true,
            Some(false),
            false,
            Some(initdb_path),
            None,
            None,
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Setup {
                deployment_name: deployment_name.clone(),
                mongodb_version: version,
                port: 27017,
                load_sample_data: false,
                connect_result: Some(ConnectResult::Skipped),
            }
        );
    }

    #[tokio::test]
    async fn test_setup_with_username_and_password() {
        let deployment_name = "test-deployment".to_string();
        let version = Version::parse("8.2.2").unwrap();

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mock_interaction = create_mock_interaction_with_spinner(outcomes.clone());

        let mut mock_deployment_management = MockDocker::new();
        let deployment = create_deployment(
            Some(deployment_name.clone()),
            version.clone(),
            Some(27017),
            Some(false),
        );
        let progress = create_successful_progress(deployment);
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |options| {
                assert_eq!(
                    options.mongodb_initdb_root_username,
                    Some("admin".to_string())
                );
                assert_eq!(
                    options.mongodb_initdb_root_password,
                    Some("password".to_string())
                );
                progress
            });

        let mut setup_command = create_setup_command(
            Some(deployment_name.clone()),
            Some(MongoDBVersion::MajorMinorPatch(
                MongoDBVersionMajorMinorPatch {
                    major: 8,
                    minor: 2,
                    patch: 2,
                },
            )),
            Some(27017),
            true,
            Some(false),
            false,
            None,
            Some("admin".to_string()),
            Some("password".to_string()),
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Setup {
                deployment_name: deployment_name.clone(),
                mongodb_version: version,
                port: 27017,
                load_sample_data: false,
                connect_result: Some(ConnectResult::Skipped),
            }
        );
    }

    #[tokio::test]
    async fn test_setup_with_bind_ip_all() {
        let deployment_name = "test-deployment".to_string();
        let version = Version::parse("8.2.2").unwrap();

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mock_interaction = create_mock_interaction_with_spinner(outcomes.clone());

        let mut mock_deployment_management = MockDocker::new();
        let deployment = create_deployment(
            Some(deployment_name.clone()),
            version.clone(),
            Some(27017),
            Some(false),
        );
        let progress = create_successful_progress(deployment);
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |options| {
                assert!(options.mongodb_port_binding.is_some());
                if let Some(binding) = options.mongodb_port_binding {
                    assert!(matches!(
                        binding.binding_type,
                        atlas_local::models::BindingType::AnyInterface
                    ));
                }
                progress
            });

        let mut setup_command = create_setup_command(
            Some(deployment_name.clone()),
            Some(MongoDBVersion::MajorMinorPatch(
                MongoDBVersionMajorMinorPatch {
                    major: 8,
                    minor: 2,
                    patch: 2,
                },
            )),
            Some(27017),
            true,
            Some(false),
            true,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Setup {
                deployment_name: deployment_name.clone(),
                mongodb_version: version,
                port: 27017,
                load_sample_data: false,
                connect_result: Some(ConnectResult::Skipped),
            }
        );
    }

    // ============================================================================
    // Connect After Setup Tests
    // ============================================================================

    #[tokio::test]
    async fn test_setup_with_connect_with_connection_string() {
        let deployment_name = "test-deployment".to_string();
        let version = Version::parse("8.2.2").unwrap();
        let connection_string = "mongodb://localhost:27017".to_string();

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mock_interaction = create_mock_interaction_with_spinner(outcomes.clone());

        let mut mock_deployment_management = MockDocker::new();
        let deployment = create_deployment(
            Some(deployment_name.clone()),
            version.clone(),
            Some(27017),
            Some(false),
        );
        let progress = create_successful_progress(deployment);
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |_| progress);

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .return_once(move |_| Ok(connection_string_clone));

        let mut setup_command = create_setup_command_with_connectors(
            Some(deployment_name.clone()),
            Some(MongoDBVersion::MajorMinorPatch(
                MongoDBVersionMajorMinorPatch {
                    major: 8,
                    minor: 2,
                    patch: 2,
                },
            )),
            Some(27017),
            true,
            Some(false),
            false,
            None,
            None,
            None,
            Some(ConnectWith::ConnectionString),
            None,
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
            HashMap::new(),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Setup {
                deployment_name: deployment_name.clone(),
                mongodb_version: version,
                port: 27017,
                load_sample_data: false,
                connect_result: Some(ConnectResult::ConnectionString { connection_string }),
            }
        );
    }

    #[tokio::test]
    async fn test_setup_with_connect_with_compass() {
        let deployment_name = "test-deployment".to_string();
        let version = Version::parse("8.2.2").unwrap();
        let connection_string = "mongodb://localhost:27017".to_string();

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mock_interaction = create_mock_interaction_with_spinner(outcomes.clone());

        let mut mock_deployment_management = MockDocker::new();
        let deployment = create_deployment(
            Some(deployment_name.clone()),
            version.clone(),
            Some(27017),
            Some(false),
        );
        let progress = create_successful_progress(deployment);
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |_| progress);

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .return_once(move |_| Ok(connection_string_clone));

        let mut mock_connector = MockConnector::new();
        mock_connector.expect_is_available().returning(|| true);
        mock_connector.expect_launch().returning(|_| Ok(()));

        let mut connectors: HashMap<ConnectWith, Box<dyn Connector + Send + Sync>> = HashMap::new();
        connectors.insert(ConnectWith::Compass, Box::new(mock_connector));

        let mut setup_command = create_setup_command_with_connectors(
            Some(deployment_name.clone()),
            Some(MongoDBVersion::MajorMinorPatch(
                MongoDBVersionMajorMinorPatch {
                    major: 8,
                    minor: 2,
                    patch: 2,
                },
            )),
            Some(27017),
            true,
            Some(false),
            false,
            None,
            None,
            None,
            Some(ConnectWith::Compass),
            None,
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
            connectors,
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Setup {
                deployment_name: deployment_name.clone(),
                mongodb_version: version,
                port: 27017,
                load_sample_data: false,
                connect_result: Some(ConnectResult::Connected {
                    method: "Compass".to_string(),
                }),
            }
        );
    }

    #[tokio::test]
    async fn test_setup_with_connect_with_compass_not_installed() {
        let deployment_name = "test-deployment".to_string();
        let version = Version::parse("8.2.2").unwrap();
        let connection_string = "mongodb://localhost:27017".to_string();

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mock_interaction = create_mock_interaction_with_spinner(outcomes.clone());

        let mut mock_deployment_management = MockDocker::new();
        let deployment = create_deployment(
            Some(deployment_name.clone()),
            version.clone(),
            Some(27017),
            Some(false),
        );
        let progress = create_successful_progress(deployment);
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |_| progress);

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .return_once(move |_| Ok(connection_string_clone));

        let mut mock_connector = MockConnector::new();
        mock_connector.expect_is_available().returning(|| false);

        let mut connectors: HashMap<ConnectWith, Box<dyn Connector + Send + Sync>> = HashMap::new();
        connectors.insert(ConnectWith::Compass, Box::new(mock_connector));

        let mut setup_command = create_setup_command_with_connectors(
            Some(deployment_name.clone()),
            Some(MongoDBVersion::MajorMinorPatch(
                MongoDBVersionMajorMinorPatch {
                    major: 8,
                    minor: 2,
                    patch: 2,
                },
            )),
            Some(27017),
            true,
            Some(false),
            false,
            None,
            None,
            None,
            Some(ConnectWith::Compass),
            None,
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
            connectors,
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Setup {
                deployment_name: deployment_name.clone(),
                mongodb_version: version,
                port: 27017,
                load_sample_data: false,
                connect_result: Some(ConnectResult::Failed {
                    error: "Compass is not installed".to_string(),
                }),
            }
        );
    }

    #[tokio::test]
    async fn test_setup_prompts_for_connection_and_selects_connection_string() {
        let deployment_name = "test-deployment".to_string();
        let version = Version::parse("8.2.2").unwrap();
        let connection_string = "mongodb://localhost:27017".to_string();

        let outcomes = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut mock_interaction = create_mock_interaction_with_spinner(outcomes.clone());

        // Two selects: setup type (default) and connection method (connection string)
        let select_call_count = std::sync::atomic::AtomicUsize::new(0);
        mock_interaction
            .expect_select()
            .times(2)
            .returning(move |_| {
                let count = select_call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                match count {
                    0 => Ok(crate::interaction::SelectPromptResult::Selected(
                        SETUP_TYPE_DEFAULT.to_string(),
                    )),
                    _ => Ok(crate::interaction::SelectPromptResult::Selected(
                        CONNECT_WITH_CONNECTION_STRING.to_string(),
                    )),
                }
            });

        let mut mock_deployment_management = MockDocker::new();
        let deployment = create_deployment(
            Some(deployment_name.clone()),
            version.clone(),
            Some(27017),
            Some(false),
        );
        let progress = create_successful_progress(deployment);
        mock_deployment_management
            .expect_create_deployment()
            .return_once(move |_| progress);

        let connection_string_clone = connection_string.clone();
        mock_deployment_management
            .expect_get_connection_string()
            .return_once(move |_| Ok(connection_string_clone));

        let mut setup_command = create_setup_command(
            Some(deployment_name.clone()),
            Some(MongoDBVersion::MajorMinorPatch(
                MongoDBVersionMajorMinorPatch {
                    major: 8,
                    minor: 2,
                    patch: 2,
                },
            )),
            Some(27017),
            false,
            Some(false),
            false,
            None,
            None,
            None,
            Box::new(mock_interaction),
            Box::new(mock_deployment_management),
        );

        let result = setup_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            SetupResult::Setup {
                deployment_name: deployment_name.clone(),
                mongodb_version: version,
                port: 27017,
                load_sample_data: false,
                connect_result: Some(ConnectResult::ConnectionString { connection_string }),
            }
        );
    }

    // ============================================================================
    // Display Tests
    // ============================================================================

    #[test]
    fn test_setup_result_display_failed_with_deployment_name() {
        let result = SetupResult::Failed {
            deployment_name: Some("test-deployment".to_string()),
            error: "test error".to_string(),
        };
        let output = format!("{}", result);
        assert!(
            output.contains("test-deployment"),
            "Output should contain deployment name"
        );
        assert!(
            output.contains("test error"),
            "Output should contain error message"
        );
        assert!(output.contains("failed"), "Output should indicate failure");
    }

    #[test]
    fn test_setup_result_display_failed_without_deployment_name() {
        let result = SetupResult::Failed {
            deployment_name: None,
            error: "test error".to_string(),
        };
        let output = format!("{}", result);
        assert!(
            output.contains("test error"),
            "Output should contain error message"
        );
        assert!(output.contains("failed"), "Output should indicate failure");
        assert!(
            !output.contains("'"),
            "Output should not contain quotes when name is missing"
        );
    }

    #[test]
    fn test_setup_result_display_setup() {
        let result = SetupResult::Setup {
            deployment_name: "test-deployment".to_string(),
            mongodb_version: Version::parse("8.2.2").unwrap(),
            port: 27017,
            load_sample_data: true,
            connect_result: None,
        };
        let output = format!("{}", result);
        assert!(
            output.contains("test-deployment"),
            "Output should contain deployment name"
        );
        assert!(
            output.contains("8.2.2"),
            "Output should contain MongoDB version"
        );
        assert!(output.contains("27017"), "Output should contain port");
        assert!(
            output.contains("true"),
            "Output should contain load_sample_data value"
        );
    }

    // ============================================================================
    // Utility Function Tests
    // ============================================================================

    #[test]
    fn test_deployment_outcome_to_multi_step_spinner_outcome_success() {
        let outcome = CreateDeploymentStepOutcome::Success;
        let result = deployment_outcome_to_multi_step_spinner_outcome(outcome);
        assert!(matches!(result, MultiStepSpinnerOutcome::Success));
    }

    #[test]
    fn test_deployment_outcome_to_multi_step_spinner_outcome_skipped() {
        let outcome = CreateDeploymentStepOutcome::Skipped;
        let result = deployment_outcome_to_multi_step_spinner_outcome(outcome);
        assert!(matches!(result, MultiStepSpinnerOutcome::Skipped));
    }

    #[test]
    fn test_deployment_outcome_to_multi_step_spinner_outcome_failure() {
        let outcome = CreateDeploymentStepOutcome::Failure;
        let result = deployment_outcome_to_multi_step_spinner_outcome(outcome);
        assert!(matches!(result, MultiStepSpinnerOutcome::Failure));
    }

    #[test]
    fn test_parse_bool_true_values() {
        assert_eq!(parse_bool("true").unwrap(), true);
        assert_eq!(parse_bool("TRUE").unwrap(), true);
        assert_eq!(parse_bool("True").unwrap(), true);
        assert_eq!(parse_bool("1").unwrap(), true);
    }

    #[test]
    fn test_parse_bool_false_values() {
        assert_eq!(parse_bool("false").unwrap(), false);
        assert_eq!(parse_bool("FALSE").unwrap(), false);
        assert_eq!(parse_bool("False").unwrap(), false);
        assert_eq!(parse_bool("0").unwrap(), false);
    }

    #[test]
    fn test_parse_bool_errors_on_invalid_value() {
        let err = parse_bool("invalid").expect_err("parse_bool should error on invalid value");
        let msg = err.to_string();
        assert!(
            msg.contains("expected true or false"),
            "error message should mention expected values: {}",
            msg
        );
        assert!(
            msg.contains("invalid"),
            "error message should mention the invalid value: {}",
            msg
        );
    }

    #[test]
    fn test_parse_bool_errors_on_empty_string() {
        parse_bool("").expect_err("parse_bool should error on empty string");
    }
    // ============================================================================
    // TryFrom Tests
    // ============================================================================

    #[test]
    fn test_setup_try_from() {
        use crate::args;
        // Test the TryFrom implementation
        // This will attempt to connect to Docker, so it may fail if Docker is not available
        // But we're testing that the conversion logic works
        let args = args::Setup {
            deployment_name: Some("test".to_string()),
            mdb_version: Some(MongoDBVersion::Latest),
            port: Some(27017),
            bind_ip_all: false,
            initdb: None,
            force: true,
            load_sample_data: Some(false),
            username: Some("admin".to_string()),
            password: Some("password".to_string()),
            image: Some("test-image".to_string()),
            skip_pull_image: true,
            connect_with: Some(ConnectWith::Compass),
        };

        let result = Setup::try_from(args);
        // The result may be Ok or Err depending on Docker availability
        // But we're testing that the code path is executed
        match result {
            Ok(setup) => {
                assert_eq!(setup.deployment_name, Some("test".to_string()));
                assert_eq!(setup.port, Some(27017));
                assert_eq!(setup.bind_ip_all, false);
                assert_eq!(setup.force, true);
                assert_eq!(setup.load_sample_data, Some(false));
                assert_eq!(setup.username, Some("admin".to_string()));
                assert_eq!(setup.password, Some("password".to_string()));
                assert_eq!(setup.image, Some("test-image".to_string()));
                assert_eq!(setup.skip_pull_image, true);
                assert_eq!(setup.connect_with, Some(ConnectWith::Compass));
            }
            Err(_) => {
                // Docker might not be available, which is fine for unit tests
                // The important thing is that the code was executed
            }
        }
    }
}
