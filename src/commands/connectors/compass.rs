use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;

use super::{Connector, DeploymentParams, launch};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Compass;

impl Compass {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Connector for Compass {
    async fn is_available(&self) -> bool {
        compass_bin().exists()
    }

    async fn launch(&self, params: &DeploymentParams) -> Result<()> {
        launch(compass_bin(), |command| {
            command.arg(&params.connection_string);
        })
        .await
    }
}

// returns the compass binary on macOS
#[cfg(target_os = "macos")]
fn compass_bin() -> PathBuf {
    PathBuf::from("/Applications/MongoDB Compass.app/Contents/MacOS/MongoDB Compass")
}

// returns the compass binary on Windows
#[cfg(target_os = "windows")]
fn compass_bin() -> PathBuf {
    use std::path::Path;
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

    const COMPASS_BIN: &str = "MongoDBCompass.exe";

    // First, try to find the path using the Windows registry.
    // Registry location: HKEY_LOCAL_MACHINE\SOFTWARE\MongoDB\MongoDB Compass
    // Registry key: Directory
    // If the registry lookup succeeds and the path exists, use it.
    // Otherwise, fall back to searching in the PATH environment variable.
    // If that also fails, return a default path (caller should check existence).
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SOFTWARE\MongoDB\MongoDB Compass", KEY_READ)
        .and_then(|key| key.get_value::<String, _>("Directory"))
        .ok()
        .map(|directory| Path::new(&directory).join(COMPASS_BIN))
        .filter(|path| path.exists())
        .or_else(|| which::which(COMPASS_BIN).ok())
        .unwrap_or_else(|| PathBuf::from(COMPASS_BIN))
}

// returns the compass binary on Linux
#[cfg(target_os = "linux")]
fn compass_bin() -> PathBuf {
    PathBuf::from("/usr/bin/mongodb-compass")
}
