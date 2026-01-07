# Atlas Local - Invocation Methods
Atlas Local can be invoked in 2 different ways:
- as a standalone cli using the `atlas-local` command
- as a MongoDB Atlas CLI plugin: `atlas local`

## Standalone
In standalone mode the usage string in the help text should be: `Usage: atlas-local <COMMAND>`.

```trycmd
$ atlas-local
? 2
Manage local deployments

Usage: atlas-local <COMMAND>

Commands:
  delete  Delete a deployment
  list    List all local deployments
  logs    Get deployment logs
  start   Start a deployment
  stop    Stop (pause) a deployment
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version

```

## Plugin
In plugin mode the usage string in the help text should be: `Usage: atlas local <COMMAND>`.

```trycmd
$ atlas local --help
...
The local plugin subcommand This is the root subcommand when executing the executable as a plugin

Usage: atlas local <COMMAND>

Commands:
  delete  Delete a deployment
  list    List all local deployments
  logs    Get deployment logs
  start   Start a deployment
  stop    Stop (pause) a deployment
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help

```