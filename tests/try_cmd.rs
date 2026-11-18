#[cfg(feature = "e2e-tests")]
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(feature = "e2e-tests")]
struct TempConfigHome(PathBuf);

#[cfg(feature = "e2e-tests")]
impl TempConfigHome {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "atlas-local-cli-e2e-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

#[cfg(feature = "e2e-tests")]
impl Drop for TempConfigHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(all(feature = "e2e-tests", windows))]
const BINARY_NAME: &str = "atlas-local.exe";
#[cfg(all(feature = "e2e-tests", not(windows)))]
const BINARY_NAME: &str = "atlas-local";

#[cfg(feature = "e2e-tests")]
#[test]
fn standalone_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_atlas-local"))
        .arg("--help")
        .output()
        .expect("run atlas-local --help");
    assert!(
        output.status.success(),
        "atlas-local --help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_help_output(&stdout, "Usage: atlas-local [OPTIONS] <COMMAND>");
}

#[cfg(feature = "e2e-tests")]
#[test]
fn atlas_plugin_help() {
    let atlas_cli_path =
        which::which("atlas").expect("Atlas CLI should be installed and in the PATH");
    let plugins_dir =
        bootstrap_atlas_cli_plugin().expect("Bootstrapping Atlas CLI plugin should not fail");
    let config_home = TempConfigHome::new().expect("create isolated Atlas CLI config home");

    let output = Command::new(atlas_cli_path)
        .args(["local", "--help"])
        // Load the test plugin from its extra directory without picking up the user's plugins.
        .env("HOME", &config_home.0)
        .env("XDG_CONFIG_HOME", &config_home.0)
        .env(
            "ATLAS_CLI_EXTRA_PLUGIN_DIRECTORY",
            plugins_dir.to_string_lossy().to_string(),
        )
        .output()
        .expect("run atlas local --help");
    assert!(
        output.status.success(),
        "atlas local --help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_help_output(&stdout, "Usage: atlas local [OPTIONS] <COMMAND>");
}

#[cfg(feature = "e2e-tests")]
fn assert_help_output(stdout: &str, expected_usage: &str) {
    assert!(stdout.contains("Manage local deployments"));
    assert!(stdout.contains(expected_usage));
    for command in [
        "setup", "connect", "list", "start", "stop", "logs", "delete", "search",
    ] {
        assert!(
            stdout
                .lines()
                .any(|line| line.split_whitespace().next() == Some(command)),
            "missing {command:?} in help output:\n{stdout}"
        );
    }
}

#[cfg(feature = "e2e-tests")]
fn bootstrap_atlas_cli_plugin() -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Build the binary
    let build_output = Command::new("cargo")
        .args(["build", "--release", "--features=generate-manifest"])
        .output()?;

    if !build_output.status.success() {
        return Err(format!(
            "cargo build failed: {}",
            String::from_utf8_lossy(&build_output.stderr)
        )
        .into());
    }

    // Get the project root directory based on the cargo environment variable CARGO_MANIFEST_DIR
    let project_root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);

    // Set up paths
    let plugins_dir = project_root.join("target/plugins");
    let atlas_local_plugin_dir = plugins_dir.join("atlas-local");

    // Create the plugins directory
    fs::create_dir_all(&atlas_local_plugin_dir)?;

    // Copy the manifest and binary
    fs::copy(
        &project_root.join("manifest.yml"),
        &atlas_local_plugin_dir.join("manifest.yml"),
    )?;
    fs::copy(
        &project_root.join("target/release/atlas-local"),
        &atlas_local_plugin_dir.join(BINARY_NAME),
    )?;

    Ok(plugins_dir)
}
