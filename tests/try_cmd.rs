use std::fs;
use std::path::PathBuf;
use std::process::Command;

use trycmd::schema::Bin;

#[cfg(feature = "e2e-tests")]
#[test]
fn try_cmd_e2e() {
    let atlas_cli_path =
        which::which("atlas").expect("Atlas CLI should be installed and in the PATH");
    let plugins_dir =
        bootstrap_atlas_cli_plugin().expect("Bootstrapping Atlas CLI plugin should not fail");

    trycmd::TestCases::new()
        .case("tests/try_cmd/*.md")
        .register_bin("atlas", Bin::Path(atlas_cli_path))
        .env(
            "ATLAS_CLI_EXTRA_PLUGIN_DIRECTORY",
            plugins_dir.to_string_lossy().to_string(),
        )
        .run();
}

fn bootstrap_atlas_cli_plugin() -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Build the binary
    let build_output = Command::new("cargo")
        .args(&["build", "--release", "--features=generate-manifest"])
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
        &atlas_local_plugin_dir.join("atlas-local"),
    )?;

    Ok(plugins_dir)
}
