# atlas-local-cli

> A CLI to manage [MongoDB Atlas local](https://hub.docker.com/repository/docker/mongodb/mongodb-atlas-local/general) environments

> [!WARNING]
> This project is a **work in progress** and is **not production ready**. APIs and functionality may change without notice.

## Overview

`atlas-local-cli` is a dedicated command-line tool focused strictly on the management of [MongoDB Atlas local](https://hub.docker.com/repository/docker/mongodb/mongodb-atlas-local/general) environments.
It provides a streamlined way to create, manage, and control local atlas instances.

### Goals

- **User Experience**: Provide a polished, intuitive interface for developers manually managing local databases.
- **Scripting Interface**: Offer a consistent and parseable interface designed specifically for automation scripts and local development pipelines.

## Installation

## As an Atlas CLI plugin
TODO

## As a standalone CLI
TODO

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

