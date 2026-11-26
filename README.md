# atlas-local-cli

> A CLI to manage [MongoDB Atlas local](https://hub.docker.com/repository/docker/mongodb/mongodb-atlas-local/general) environments

## Overview

`atlas-local-cli` is a dedicated command-line tool focused strictly on the management of [MongoDB Atlas local](https://hub.docker.com/repository/docker/mongodb/mongodb-atlas-local/general) environments.
It provides a streamlined way to create, manage, and control local atlas instances.

### Goals

- **User Experience**: Provide a polished, intuitive interface for developers manually managing local databases.
- **Scripting Interface**: Offer a consistent and parseable interface designed specifically for automation scripts and local development pipelines.

## Installation

## As an Atlas CLI plugin
The Atlas Local CLI is installed as a first-class plugin inside the Atlas CLI. The plugin will automatically be installed on the first call of the command.

You can run the commands by running `atlas local *`

For additional Docker container options, set `MONGODB_ATLAS_LOCAL_DOCKER_PARAMS` to a JSON object of Docker `HostConfig` properties. For example, to set the `nofile` ulimit:

```bash
MONGODB_ATLAS_LOCAL_DOCKER_PARAMS='{"Ulimits":[{"Name":"nofile","Soft":64000,"Hard":64000}]}' atlas local setup
```

Provided `PortBindings` and `Binds` replace Atlas Local's generated values. Omit either property or set it to `null` to keep its default. Other `HostConfig` fields are passed through.

## As a standalone CLI
See the instructions on the releases page: https://github.com/mongodb/atlas-local-cli/releases/

### From source

```bash
git clone https://github.com/mongodb/atlas-local-cli
cd atlas-local-cli
cargo install --path .
```

### Examples

Check out the [`examples/`](examples/) directory for usage examples. You can run them with:

```bash
cargo run --example [todo]
```

## Development

### Releasing

See [RELEASING.md](RELEASING.md) for the release process.

### Building

```bash
cargo build
```

### Running tests

```bash
cargo test
```

## License

See [LICENSE](LICENSE) for details.
