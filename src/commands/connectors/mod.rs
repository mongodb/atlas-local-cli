use async_trait::async_trait;
use std::{
    env,
    ffi::OsStr,
    process::{Stdio, exit},
};
use tokio::process::Command;

use anyhow::Result;

mod compass;
mod mongosh;
mod vscode;

pub use compass::Compass;
pub use mongosh::Mongosh;
pub use vscode::VsCode;

#[async_trait]
pub trait Connector {
    async fn is_available(&self) -> bool;
    async fn launch(&self, params: &DeploymentParams) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentParams {
    pub name: String,
    pub connection_string: String,
}

impl DeploymentParams {
    pub fn new(name: impl Into<String>, connection_string: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            connection_string: connection_string.into(),
        }
    }
}

async fn launch<F, P>(bin: P, customizer: F) -> Result<()>
where
    P: AsRef<OsStr>,
    F: FnOnce(&mut Command),
{
    let mut command = Command::new(bin);
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());
    command.envs(env::vars());

    customizer(&mut command);
    let status = command.status().await?;

    if !status.success() {
        exit(status.code().unwrap_or(1));
    }

    Ok(())
}
