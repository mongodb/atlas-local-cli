use std::{fmt::Display, path::PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use atlas_local::{
    Client, CreateDeploymentError,
    client::CreateDeploymentStepOutcome,
    models::{
        BindingType, CreateDeploymentOptions, CreationSource, MongoDBPortBinding, MongoDBVersion,
        MongoDBVersionMajor, MongoDBVersionMajorMinor, MongoDBVersionMajorMinorPatch,
    },
};
use bollard::Docker;
use semver::Version;
use serde::Serialize;
use tracing::debug;

use crate::{
    args,
    commands::CommandWithOutput,
    dependencies::DeploymentCreator,
    interaction::{
        InputPrompt, InputPromptOptions, InputPromptResult, InputPromptValidator, InputValidator,
        InputValidatorResult, Interaction, MultiStepSpinnerInteraction, MultiStepSpinnerOutcome,
        MultiStepSpinnerStep, SelectPrompt, SelectPromptOptions, SelectPromptResult,
        SpinnerInteraction,
    },
};

// Setup dependencies for the setup command
pub trait SetupDeploymentManagement: DeploymentCreator {}
impl<T: DeploymentCreator> SetupDeploymentManagement for T {}

// Interaction dependencies for the setup command
pub trait SetupInteraction:
    SpinnerInteraction + SelectPrompt + InputPrompt + MultiStepSpinnerInteraction
{
}
impl<T: SpinnerInteraction + SelectPrompt + InputPrompt + MultiStepSpinnerInteraction>
    SetupInteraction for T
{
}

pub struct Setup {
    deployment_name: Option<String>,
    mdb_version: Option<MongoDBVersion>,
    port: Option<u16>,
    bind_ip_all: bool,
    initdb: Option<PathBuf>,
    force: bool,
    load_sample_data: Option<bool>,
    username: Option<String>,
    password: Option<String>,

    image: Option<String>,
    skip_pull_image: bool,

    interaction: Box<dyn SetupInteraction + Send>,
    deployment_management: Box<dyn SetupDeploymentManagement + Send>,
}

impl TryFrom<args::Setup> for Setup {
    type Error = anyhow::Error;

    fn try_from(args: args::Setup) -> Result<Self> {
        Ok(Self {
            deployment_name: args.deployment_name,
            mdb_version: args.mdb_version,
            port: args.port,
            bind_ip_all: args.bind_ip_all,
            initdb: args.initdb,
            force: args.force,
            load_sample_data: args.load_sample_data,
            username: args.username,
            password: args.password,
            image: args.image,
            skip_pull_image: args.skip_pull_image,

            interaction: Box::new(Interaction::new()),
            deployment_management: Box::new(Client::new(
                Docker::connect_with_defaults().context("connecting to Docker")?,
            )),
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
    },
    Failed {
        deployment_name: Option<String>,
        error: String,
    },
}

impl Display for SetupResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Setup {
                deployment_name,
                mongodb_version,
                port,
                load_sample_data,
            } => {
                writeln!(f, "Successfully setup deployment '{deployment_name}'")?;
                writeln!(f, "MongoDB version: {mongodb_version}")?;
                writeln!(f, "Port: {port}")?;
                write!(f, "Load sample data: {load_sample_data}")?;
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
            Ok(deployment) => Ok(SetupResult::Setup {
                deployment_name: deployment.name.unwrap_or("unknown".to_string()),
                mongodb_version: deployment.mongodb_version,
                port: deployment
                    .port_bindings
                    .as_ref()
                    .and_then(|bindings| bindings.port)
                    .unwrap_or(0),
                load_sample_data: deployment.mongodb_load_sample_data.unwrap_or(false),
            }),
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
            DeploymentNameValidator,
        )?;

        if let PromptCustomSettingsResult::Canceled = prompt_deployment_name_result {
            return Ok(PromptCustomSettingsResult::Canceled);
        }

        // Prompt for the MongoDB version
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
            MdbVersionValidator,
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
            PortValidator,
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
                    yes_no_to_bool(answer.as_str(), false)
                        .map_err(|e| anyhow::anyhow!("converting yes/no to bool: {}", e))?,
                );

                Ok(())
            },
            YesNoValidator,
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
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum PromptCustomSettingsResult {
    Continue,
    Canceled,
}

#[derive(Clone)]
struct DeploymentNameValidator;

impl InputValidator for DeploymentNameValidator {
    fn validate(&self, _: &str) -> Result<crate::interaction::InputValidatorResult> {
        Ok(InputValidatorResult::Valid)
    }
}

#[derive(Clone)]
struct MdbVersionValidator;

impl InputValidator for MdbVersionValidator {
    fn validate(&self, input: &str) -> Result<InputValidatorResult> {
        match MongoDBVersion::try_from(input) {
            Ok(v) => match v {
                MongoDBVersion::Latest => Ok(InputValidatorResult::Valid),
                MongoDBVersion::Major(MongoDBVersionMajor { major })
                | MongoDBVersion::MajorMinor(MongoDBVersionMajorMinor { major, .. })
                | MongoDBVersion::MajorMinorPatch(MongoDBVersionMajorMinorPatch {
                    major, ..
                }) => {
                    if major < 7 {
                        Ok(InputValidatorResult::Invalid(
                            "The lowest supported MongoDB version is 7".to_string(),
                        ))
                    } else {
                        Ok(InputValidatorResult::Valid)
                    }
                }
            },
            Err(e) => Ok(InputValidatorResult::Invalid(e.to_string())),
        }
    }
}

#[derive(Clone)]
struct PortValidator;
const PORT_ERROR_MESSAGE: &str =
    "Port must be a number between 1 and 65535, leave empty to auto-assign";

impl InputValidator for PortValidator {
    fn validate(&self, input: &str) -> Result<InputValidatorResult> {
        let invalid_port_result = || {
            Ok(InputValidatorResult::Invalid(
                PORT_ERROR_MESSAGE.to_string(),
            ))
        };

        if input.is_empty() || input == "auto-assign" {
            return Ok(InputValidatorResult::Valid);
        }

        match input.parse::<u16>() {
            Ok(port) => {
                if port < 1 {
                    return invalid_port_result();
                }
                Ok(InputValidatorResult::Valid)
            }
            Err(_) => invalid_port_result(),
        }
    }
}

#[derive(Clone)]
struct YesNoValidator;
impl InputValidator for YesNoValidator {
    fn validate(&self, input: &str) -> Result<InputValidatorResult> {
        match yes_no_to_bool(input, false) {
            Ok(_) => Ok(InputValidatorResult::Valid),
            Err(e) => Ok(InputValidatorResult::Invalid(e)),
        }
    }
}

fn yes_no_to_bool(input: &str, default: bool) -> Result<bool, String> {
    match input.to_lowercase().as_str() {
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        "" => Ok(default),
        _ => Err(format!("Invalid input '{input}', please enter y or n")),
    }
}

const SETUP_TYPE_DEFAULT: &str = "With default settings";
const SETUP_TYPE_CUSTOM: &str = "With custom settings";
const SETUP_TYPE_CANCEL: &str = "Cancel setup";
