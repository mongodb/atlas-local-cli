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
The Atlas Local CLI is installed as a first-class plugin inside the Atlas CLI.

You can run the commands by running `atlas local *`

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

