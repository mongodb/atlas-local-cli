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

Usage: atlas-local [OPTIONS] <COMMAND>

Commands:
  setup    Create a local deployment
  connect  Connect to a deployment
  list     List all local deployments
  start    Start a deployment
  stop     Stop (pause) a deployment
  logs     Get deployment logs
  delete   Delete a deployment
  search   Manage search for local deployments.
  help     Print this message or the help of the given subcommand(s)

Options:
  -o, --output <FORMAT>    Output format [possible values: text, json]
  -P, --profile <PROFILE>  Name of the profile to use from your configuration file. To learn about profiles for the Atlas CLI, see https://dochub.mongodb.org/core/atlas-cli-save-connection-settings
  -h, --help               Print help
  -V, --version            Print version

```

## Plugin
In plugin mode the usage string in the help text should be: `Usage: atlas local <COMMAND>`.

```trycmd
$ atlas local --help
The local plugin subcommand This is the root subcommand when executing the executable as a plugin

Usage: atlas local [OPTIONS] <COMMAND>

Commands:
  setup    Create a local deployment
  connect  Connect to a deployment
  list     List all local deployments
  start    Start a deployment
  stop     Stop (pause) a deployment
  logs     Get deployment logs
  delete   Delete a deployment
  search   Manage search for local deployments.
  help     Print this message or the help of the given subcommand(s)

Options:
  -o, --output <FORMAT>    Output format [possible values: text, json]
  -P, --profile <PROFILE>  Name of the profile to use from your configuration file. To learn about profiles for the Atlas CLI, see https://dochub.mongodb.org/core/atlas-cli-save-connection-settings
  -h, --help               Print help

```