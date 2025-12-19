//! This module defines traits for external dependencies (such as Docker interactions) to make them
//! easier to mock and substitute in tests or other environments. By abstracting external services
//! behind traits, components can be decoupled and dependency-injected, improving testability and maintainability.
use async_trait::async_trait;
use atlas_local::{
    Client, GetLogsError,
    models::{LogOutput, LogsOptions},
};

#[cfg(test)]
pub use mocks::*;

// Dependency to list deployments
#[async_trait]
pub trait DeploymentLister {
    /// Returns a list of all local deployments.
    async fn list(
        &self,
    ) -> Result<Vec<atlas_local::models::Deployment>, atlas_local::GetDeploymentError>;
}

#[async_trait]
impl DeploymentLister for Client {
    async fn list(
        &self,
    ) -> Result<Vec<atlas_local::models::Deployment>, atlas_local::GetDeploymentError> {
        self.list_deployments().await
    }
}

#[async_trait]
pub trait DeploymentDeleter {
    async fn delete(&self, deployment_name: &str)
    -> Result<(), atlas_local::DeleteDeploymentError>;
}

#[async_trait]
impl DeploymentDeleter for Client {
    async fn delete(
        &self,
        deployment_name: &str,
    ) -> Result<(), atlas_local::DeleteDeploymentError> {
        self.delete_deployment(deployment_name).await
    }
}

// Dependency to get deployment logs
#[async_trait]
pub trait DeploymentLogsRetriever {
    async fn get_logs(
        &self,
        container_id_or_name: &str,
        options: Option<LogsOptions>,
    ) -> Result<Vec<LogOutput>, GetLogsError>;
}

#[async_trait]
impl DeploymentLogsRetriever for Client {
    async fn get_logs(
        &self,
        container_id_or_name: &str,
        options: Option<LogsOptions>,
    ) -> Result<Vec<LogOutput>, GetLogsError> {
        self.get_logs(container_id_or_name, options).await
    }
}

#[cfg(test)]
pub mod mocks {
    use super::*;
    use mockall::mock;

    mock! {
        pub Docker {}

        #[async_trait]
        impl DeploymentLister for Docker {
            async fn list(&self) -> Result<Vec<atlas_local::models::Deployment>, atlas_local::GetDeploymentError>;
        }

        #[async_trait]
        impl DeploymentDeleter for Docker {
            async fn delete(&self, deployment_name: &str) -> Result<(), atlas_local::DeleteDeploymentError>;
        }

        #[async_trait]
        impl DeploymentLogsRetriever for Docker {
            async fn get_logs(&self, container_id_or_name: &str, options: Option<LogsOptions>) -> Result<Vec<LogOutput>, GetLogsError>;
        }
    }
}
