//! This module defines traits for external dependencies (such as Docker interactions) to make them
//! easier to mock and substitute in tests or other environments. By abstracting external services
//! behind traits, components can be decoupled and dependency-injected, improving testability and maintainability.
use async_trait::async_trait;
use atlas_local::{
    Client, GetDeploymentError, GetLogsError,
    client::{
        CreateDeploymentProgress, StartDeploymentError, StopDeploymentError,
        UnpauseDeploymentError, WatchDeploymentError,
    },
    models::{CreateDeploymentOptions, Deployment, LogOutput, LogsOptions, WatchOptions},
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

#[async_trait]
pub trait DeploymentStarter {
    async fn start(&self, deployment_name: &str) -> Result<(), StartDeploymentError>;
}

#[async_trait]
impl DeploymentStarter for Client {
    async fn start(&self, deployment_name: &str) -> Result<(), StartDeploymentError> {
        self.start_deployment(deployment_name).await
    }
}

#[async_trait]
pub trait DeploymentGetDeployment {
    async fn get_deployment(&self, deployment_name: &str)
    -> Result<Deployment, GetDeploymentError>;
}

#[async_trait]
impl DeploymentGetDeployment for Client {
    async fn get_deployment(
        &self,
        deployment_name: &str,
    ) -> Result<Deployment, GetDeploymentError> {
        self.get_deployment(deployment_name).await
    }
}

#[async_trait]
pub trait DeploymentUnpauser {
    async fn unpause(&self, deployment_name: &str) -> Result<(), UnpauseDeploymentError>;
}

#[async_trait]
impl DeploymentUnpauser for Client {
    async fn unpause(&self, deployment_name: &str) -> Result<(), UnpauseDeploymentError> {
        self.unpause_deployment(deployment_name).await
    }
}

#[async_trait]
pub trait DeploymentStopper {
    async fn stop(&self, deployment_name: &str) -> Result<(), StopDeploymentError>;
}

#[async_trait]
impl DeploymentStopper for Client {
    async fn stop(&self, deployment_name: &str) -> Result<(), StopDeploymentError> {
        self.stop_deployment(deployment_name).await
    }
}

#[async_trait]
pub trait DeploymentWaiter {
    async fn wait_for_healthy_deployment(
        &self,
        deployment_name: &str,
        options: WatchOptions,
    ) -> Result<(), WatchDeploymentError>;
}

#[async_trait]
impl DeploymentWaiter for Client {
    async fn wait_for_healthy_deployment(
        &self,
        deployment_name: &str,
        options: WatchOptions,
    ) -> Result<(), WatchDeploymentError> {
        self.wait_for_healthy_deployment(deployment_name, options)
            .await
    }
}

pub trait DeploymentCreator {
    fn create_deployment(
        &self,
        deployment_options: CreateDeploymentOptions,
    ) -> CreateDeploymentProgress;
}

impl DeploymentCreator for Client {
    fn create_deployment(
        &self,
        deployment_options: CreateDeploymentOptions,
    ) -> CreateDeploymentProgress {
        self.create_deployment(deployment_options)
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

        #[async_trait]
        impl DeploymentStarter for Docker {
            async fn start(&self, deployment_name: &str) -> Result<(), StartDeploymentError>;
        }

        #[async_trait]
        impl DeploymentGetDeployment for Docker {
            async fn get_deployment(&self, deployment_name: &str) -> Result<Deployment, GetDeploymentError>;
        }

        #[async_trait]
        impl DeploymentUnpauser for Docker {
            async fn unpause(&self, deployment_name: &str) -> Result<(), UnpauseDeploymentError>;
        }

        #[async_trait]
        impl DeploymentStopper for Docker {
            async fn stop(&self, deployment_name: &str) -> Result<(), StopDeploymentError>;
        }

        #[async_trait]
        impl DeploymentWaiter for Docker {
            async fn wait_for_healthy_deployment(&self, deployment_name: &str, options: WatchOptions) -> Result<(), WatchDeploymentError>;
        }
    }
}
