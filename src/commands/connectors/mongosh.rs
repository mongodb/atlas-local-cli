use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use which::which;

use super::{Connector, DeploymentParams, launch};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Mongosh;

impl Mongosh {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Connector for Mongosh {
    async fn is_available(&self) -> bool {
        mongosh_bin().await.is_some_and(|path| path.exists())
    }

    async fn launch(&self, params: &DeploymentParams) -> Result<()> {
        let bin = mongosh_bin().await.context("mongosh not found")?;
        launch(bin, |command| {
            command.arg(&params.connection_string);
        })
        .await
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
const MONGOSH_BIN: &str = "mongosh";

#[cfg(target_os = "windows")]
const MONGOSH_BIN: &str = "mongosh.exe";

async fn mongosh_bin() -> Option<PathBuf> {
    which(MONGOSH_BIN).ok()
}
