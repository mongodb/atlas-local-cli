# Releasing

1. Run **Prepare Release** from the `main` branch in GitHub Actions. Enter `patch`, `minor`, `major`, or an exact version.
2. Review and approve the generated `release/prepare-v<version>` pull request. It updates `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, and `LICENSE-3RD-PARTY.txt`. Auto-merge completes it after required checks pass.
3. After the PR is merged, **Tag Release** creates the matching `v<version>` tag on the merge commit. The tag starts the `release.yml` workflow, which builds and publishes the release.
4. Check the release workflow and the published GitHub release. The post-announce workflow also opens a PR to update the Atlas Local plugin's minimum version in `mongodb/mongodb-atlas-cli`, with auto-merge armed after approval.

The release preparation workflow does not push commits directly to `main`. The version changes must pass the repository's normal PR checks and review before the release tag is created.
