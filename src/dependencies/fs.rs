use std::path::Path;

use anyhow::{Context, Result};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct TokioFs;

impl TokioFs {
    pub fn new() -> Self {
        Self
    }
}

// Dependency to list deployments
#[async_trait]
pub trait FileReader {
    async fn read_to_string(&self, path: &Path) -> Result<String>;
}

#[async_trait]
impl FileReader for TokioFs {
    async fn read_to_string(&self, path: &Path) -> Result<String> {
        tokio::fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to read file: {}", path.display()))
    }
}
#[cfg(test)]
pub mod mocks {
    use super::*;
    use mockall::mock;

    mock! {
        pub TokioFs {}

        #[async_trait]
        impl FileReader for TokioFs {
            async fn read_to_string(&self, path: &Path) -> Result<String>;
        }
    }
}
