#[cfg(feature = "e2e-tests")]
use std::{fs, path::PathBuf, process::Command};

#[cfg(all(feature = "e2e-tests", windows))]
const BINARY_NAME: &str = "atlas-local.exe";
#[cfg(all(feature = "e2e-tests", not(windows)))]
const BINARY_NAME: &str = "atlas-local";

#[cfg(feature = "e2e-tests")]
#[test]
fn standalone_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_atlas-local"))
        .output()
        .expect("run atlas-local");
    assert!(
        output.status.success(),
        "atlas-local invocation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let help = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let help = String::from_utf8_lossy(help);
    assert_help_output(&help, "Usage: atlas-local [OPTIONS] <COMMAND>", true);
}

#[cfg(feature = "e2e-tests")]
#[test]
fn atlas_plugin_help() {
    let atlas_cli_path =
        which::which("atlas").expect("Atlas CLI should be installed and in the PATH");
    let plugins_dir =
        bootstrap_atlas_cli_plugin().expect("Bootstrapping Atlas CLI plugin should not fail");

    let output = Command::new(atlas_cli_path)
        .args(["local", "--help"])
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
    assert_help_output(&stdout, "Usage: atlas local [OPTIONS] <COMMAND>", false);
}

#[cfg(feature = "e2e-tests")]
fn assert_help_output(stdout: &str, expected_usage: &str, has_version_option: bool) {
    let mut expected_lines = vec![
        "Manage local deployments",
        expected_usage,
        "Commands:",
        "  setup    Create a local deployment",
        "  connect  Connect to a deployment",
        "  list     List all local deployments",
        "  start    Start a deployment",
        "  stop     Stop (pause) a deployment",
        "  logs     Get deployment logs",
        "  delete   Delete a deployment",
        "  search   Manage search for local deployments.",
        "  help     Print this message or the help of the given subcommand(s)",
        "Options:",
        "  -o, --output <FORMAT>    Output format [possible values: text, json]",
        "  -P, --profile <PROFILE>  Name of the profile to use from your configuration file. To learn about profiles for the Atlas CLI, see https://dochub.mongodb.org/core/atlas-cli-save-connection-settings",
        "  -h, --help               Print help",
    ];
    if has_version_option {
        expected_lines.push("  -V, --version            Print version");
    }

    let mut remaining_lines = stdout.lines();
    for expected in expected_lines {
        assert!(
            remaining_lines.any(|line| line == expected),
            "missing or out-of-order line {expected:?} in help output:\n{stdout}"
        );
    }
    if !has_version_option {
        assert!(
            !stdout
                .lines()
                .any(|line| line == "  -V, --version            Print version"),
            "unexpected version option in help output:\n{stdout}"
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
