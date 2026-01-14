use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use url::Url;
use which::which;

use super::{Connector, DeploymentParams, launch};

pub struct VsCode;

impl VsCode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Connector for VsCode {
    async fn is_available(&self) -> bool {
        vscode_bin().await.is_some_and(|path| path.exists())
    }

    async fn launch(&self, params: &DeploymentParams) -> Result<()> {
        let bin = vscode_bin().await.context("vscode not found")?;
        let deeplink = build_deeplink(&params.connection_string, &params.name);

        launch(bin, |command| {
            command.arg("--open-url");
            command.arg(&deeplink);
        })
        .await
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
const VSCODE_BIN: &str = "code";

#[cfg(target_os = "windows")]
const VSCODE_BIN: &str = "code.exe";

async fn vscode_bin() -> Option<PathBuf> {
    which(VSCODE_BIN).ok()
}

fn build_deeplink(connection_string: &str, name: &str) -> String {
    // Safe to unwrap because the URL is hardcoded and valid
    let mut url = Url::parse("vscode://mongodb.mongodb-vscode/connectWithURI")
        .expect("hardcoded URL is valid");

    url.query_pairs_mut()
        .append_pair("connectionString", connection_string)
        .append_pair("name", format!("{} (Local)", name).as_str())
        .append_pair("reuseExisting", "true")
        .append_pair("utm_source", "atlas-local-cli");

    url.to_string()
}
