## [0.11.3] - 2026-11-16

### 🐛 Bug Fixes

- Bump atlas-local to 0.7.1, drop direct bollard dependency ([#70](https://github.com/mongodb/atlas-local-cli/pull/70))
- Suppress Compass stderr noise on connect ([#72](https://github.com/mongodb/atlas-local-cli/pull/72))
- *(deps)* Update crossbeam-epoch ([#87](https://github.com/mongodb/atlas-local-cli/pull/87))
- *(deps)* Resolve security audit advisories ([#118](https://github.com/mongodb/atlas-local-cli/pull/118))
- *(deps)* Add support for embedding_provider_endpoint and 9.0 ([#119](https://github.com/mongodb/atlas-local-cli/pull/119))

### 💼 Other

- Target master instead of main when bumping Atlas Local Plugin Min Version on atlas-cli-core release ([#120](https://github.com/mongodb/atlas-local-cli/pull/120))

### ⚙️ Miscellaneous Tasks

- Add Jira automation workflows from apix-action ([#76](https://github.com/mongodb/atlas-local-cli/pull/76))
- Apix-action updates ([#86](https://github.com/mongodb/atlas-local-cli/pull/86))
- Apix-action updates ([#90](https://github.com/mongodb/atlas-local-cli/pull/90))
- Apix-action updates ([#95](https://github.com/mongodb/atlas-local-cli/pull/95))
- Apix-action updates ([#97](https://github.com/mongodb/atlas-local-cli/pull/97))
- Apix-action updates ([#107](https://github.com/mongodb/atlas-local-cli/pull/107))
- Move third-party license generation into the Dependabot post-update hook ([#108](https://github.com/mongodb/atlas-local-cli/pull/108))
- Apix-action updates ([#109](https://github.com/mongodb/atlas-local-cli/pull/109))
- Apix-action updates ([#110](https://github.com/mongodb/atlas-local-cli/pull/110))
- Apix-action updates ([#111](https://github.com/mongodb/atlas-local-cli/pull/111))
- Apix-action updates ([#114](https://github.com/mongodb/atlas-local-cli/pull/114))
## [0.11.2] - 2026-04-21

### 🐛 Bug Fixes

- Bump apix-action version and add main cli repo ([#62](https://github.com/mongodb/atlas-local-cli/pull/62))

### ⚙️ Miscellaneous Tasks

- Update CODEOWNERS to apix-devtools ([#61](https://github.com/mongodb/atlas-local-cli/pull/61))
## [0.11.1] - 2026-04-15

### 🚀 Features

- Bump atlas local plugin MinVersion in mongodb-atlas-cli automatically ([#44](https://github.com/mongodb/atlas-local-cli/pull/44))
- Support image tag format for --mdb-version flag ([#50](https://github.com/mongodb/atlas-local-cli/pull/50))

### 🐛 Bug Fixes

- *(release)* Remove atlas-local-docs-generator from package ([#54](https://github.com/mongodb/atlas-local-cli/pull/54))
- Update rustls-webpki to 0.103.12 (RUSTSEC-2026-0098) ([#60](https://github.com/mongodb/atlas-local-cli/pull/60))

### ⚙️ Miscellaneous Tasks

- Change release input to allow patch, minor, major ([#46](https://github.com/mongodb/atlas-local-cli/pull/46))
- Use apix-bot instead of github actions to update dependabot prs ([#56](https://github.com/mongodb/atlas-local-cli/pull/56))
- Sign windows binaries ([#57](https://github.com/mongodb/atlas-local-cli/pull/57))
## [0.11.0] - 2026-03-03

### 🚀 Features

- Use preview tag when MONGODB_ATLAS_LOCAL_PREVIEW is set ([#40](https://github.com/mongodb/atlas-local-cli/pull/40))
## [0.10.0] - 2026-02-11

### 🐛 Bug Fixes

- *(build)* Remove trailing slash from repository URL in manifest generation ([#37](https://github.com/mongodb/atlas-local-cli/pull/37))

### ⚙️ Miscellaneous Tasks

- Return error code 0 for commands that display usage texts or the application version ([#38](https://github.com/mongodb/atlas-local-cli/pull/38))
- Revise README for Atlas Local CLI installation details ([#39](https://github.com/mongodb/atlas-local-cli/pull/39))
## [0.0.9] - 2026-02-10

### 🐛 Bug Fixes

- *(connect)* Output raw connection string without prefix ([#36](https://github.com/mongodb/atlas-local-cli/pull/36))
## [0.0.8] - 2026-02-03

### ⚙️ Miscellaneous Tasks

- Make .sig files armored ([#34](https://github.com/mongodb/atlas-local-cli/pull/34))
## [0.0.7] - 2026-02-03

### ⚙️ Miscellaneous Tasks

- Fix missing write permissions in 'sign-zip' workflow ([#33](https://github.com/mongodb/atlas-local-cli/pull/33))
## [0.0.6] - 2026-01-30

### ⚙️ Miscellaneous Tasks

- Fix zip signing ([#31](https://github.com/mongodb/atlas-local-cli/pull/31))
## [0.0.5] - 2026-01-30

### ⚙️ Miscellaneous Tasks

- Sign zip artifacts ([#30](https://github.com/mongodb/atlas-local-cli/pull/30))
## [0.0.4] - 2026-01-30

### 🚀 Features

- Atlas search indexes create ([#25](https://github.com/mongodb/atlas-local-cli/pull/25))
- Implement search indexes list/delete/describe ([#26](https://github.com/mongodb/atlas-local-cli/pull/26))

### 📚 Documentation

- Auto-generate docs from clap ([#29](https://github.com/mongodb/atlas-local-cli/pull/29))

### ⚙️ Miscellaneous Tasks

- Rename all flags from --kebab-case to --camelCase to be backwards compatible with the atlas cli ([#28](https://github.com/mongodb/atlas-local-cli/pull/28))
## [0.0.3] - 2026-01-14

### 🚀 Features

- Implemented list command ([#13](https://github.com/mongodb/atlas-local-cli/pull/13))
- Implement delete command for local deployments ([#14](https://github.com/mongodb/atlas-local-cli/pull/14))
- Added atlas-local log command ([#15](https://github.com/mongodb/atlas-local-cli/pull/15))
- Added start command ([#19](https://github.com/mongodb/atlas-local-cli/pull/19))
- Stop command ([#20](https://github.com/mongodb/atlas-local-cli/pull/20))
- Implemented atlas-local setup ([#21](https://github.com/mongodb/atlas-local-cli/pull/21))
- Implemented atlas-local connect ([#22](https://github.com/mongodb/atlas-local-cli/pull/22))
- Add support for --debug and --format ([#23](https://github.com/mongodb/atlas-local-cli/pull/23))

### ⚙️ Miscellaneous Tasks

- Improve dependabot-auto-approve.yml ([#11](https://github.com/mongodb/atlas-local-cli/pull/11))
## [0.0.2] - 2025-12-05

### ⚙️ Miscellaneous Tasks

- Initial repository + ci setup ([#1](https://github.com/mongodb/atlas-local-cli/pull/1))
- Disable auto updating github actions ([#8](https://github.com/mongodb/atlas-local-cli/pull/8))
- Setup release process ([#7](https://github.com/mongodb/atlas-local-cli/pull/7))
