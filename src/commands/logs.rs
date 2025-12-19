//! Command to get deployment logs.
//!
//! This module implements the `logs` command which retrieves and displays logs for a MongoDB Atlas local deployment.

use std::fmt::Display;

use anyhow::{Context, Result};
use async_trait::async_trait;
use atlas_local::{Client, models::LogsOptions};
use bollard::Docker;
use serde::Serialize;

use crate::{args, commands::CommandWithOutput, dependencies::DeploymentLogsRetriever};

/// Command to get deployment logs.
pub struct Logs {
    deployment_name: String,
    deployment_logs_retriever: Box<dyn DeploymentLogsRetriever + Send>,
}

/// Convert CLI arguments to command with default dependencies injected.
///
/// This implementation creates a new `Logs` command with the default `atlas_local::Client`
/// as the logs retriever.
impl TryFrom<args::Logs> for Logs {
    type Error = anyhow::Error;

    fn try_from(args: args::Logs) -> std::result::Result<Self, Self::Error> {
        Ok(Logs {
            deployment_name: args.deployment_name,
            deployment_logs_retriever: Box::new(Client::new(
                Docker::connect_with_defaults().context("connecting to Docker")?,
            )),
        })
    }
}

/// Result of the logs command.
///
/// We're using a newtype pattern to wrap the vector of log lines.
/// This allows us to implement traits like [`Display`] and [`Serialize`] on the result
/// without implementing them directly on `Vec<String>`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LogsResult(Vec<String>);

/// Format the logs result for display.
///
/// This implementation joins the log lines with newlines for text output.
impl Display for LogsResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.join("\n"))
    }
}

/// Execute the logs command and return the result.
///
/// This implementation calls the client to get logs for the deployment,
/// filters to only include stdout/stderr logs, and wraps them in a [`LogsResult`].
///
#[async_trait]
impl CommandWithOutput for Logs {
    type Output = LogsResult;

    async fn execute(&mut self) -> Result<Self::Output> {
        // Build the logs options.
        // We're only interested in stdout and stderr logs.
        let logs_options = LogsOptions::builder().stdout(true).stderr(true).build();

        // Get the logs from the deployment.
        let log_outputs = self
            .deployment_logs_retriever
            .get_logs(&self.deployment_name, Some(logs_options))
            .await
            .context("retrieving deployment logs")?;

        // The logs are returned as a vector of bytes, so we need to convert them to strings.
        // The logs also include trailing newlines, so we need to trim them.
        let logs: Vec<String> = log_outputs
            .into_iter()
            .map(|l| l.as_str_lossy().trim_end().to_string())
            .collect();

        Ok(LogsResult(logs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependencies::MockDocker;
    use atlas_local::models::LogOutput;
    use bytes::Bytes;

    #[tokio::test]
    async fn test_logs_command() {
        let mut deployment_logs_retriever = MockDocker::new();
        deployment_logs_retriever
            .expect_get_logs()
            .withf(|name, options| {
                name == "test-deployment"
                    && options.is_some()
                    && options.as_ref().unwrap().stdout == true
                    && options.as_ref().unwrap().stderr == true
            })
            .return_once(|_, _| {
                Ok(vec![
                    LogOutput::StdOut {
                        message: Bytes::from("First log line\n"),
                    },
                    LogOutput::StdErr {
                        message: Bytes::from("Error log line\n"),
                    },
                    LogOutput::StdOut {
                        message: Bytes::from("Second log line\n"),
                    },
                ])
            });

        let mut logs_command = Logs {
            deployment_name: "test-deployment".to_string(),
            deployment_logs_retriever: Box::new(deployment_logs_retriever),
        };

        let result = logs_command
            .execute()
            .await
            .expect("execute should succeed");

        assert_eq!(
            result,
            LogsResult(vec![
                "First log line".to_string(),
                "Error log line".to_string(),
                "Second log line".to_string(),
            ])
        );
    }
}
