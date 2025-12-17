//! This module defines traits for external dependencies (such as Docker interactions) to make them
//! easier to mock and substitute in tests or other environments. By abstracting external services
//! behind traits, components can be decoupled and dependency-injected, improving testability and maintainability.
use async_trait::async_trait;
use atlas_local::Client;

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
    }
}
