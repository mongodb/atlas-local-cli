use std::{any::TypeId, collections::BTreeMap, fmt::Write, fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result, bail};
use args::Cli;
use atlas_local::models::MongoDBVersion;
use clap::{CommandFactory, builder::ValueParser};

pub mod args;
pub mod formatting;

const OUTPUT_DIR: &str = "docs/generated";

const TOC: &str = r#"
.. default-domain:: mongodb

.. contents:: On this page
   :local:
   :backlinks: none
   :depth: 1
   :class: singlecol
"#;

const SYNTAX_HEADER: &str = r#"Syntax
------

.. code-block::
   :caption: Command Syntax
"#;

const TOCTREE_HEADER: &str = r#"
.. toctree::
   :titlesonly:
"#;

fn main() -> Result<()> {
    let mut root = Cli::command();

    // Only make the "local" command visible, hide all other subcommands.
    // This way the documentation we generate will always start with the "local" command. (atlas local ...)
    root.get_subcommands_mut().for_each(|c| {
        let should_hide = c.get_name() != "local";
        // We need to clone the command because `hide` consumes the command.
        // We can't consume a command when we only have a reference to it.
        *c = c.clone().hide(should_hide);
    });

    let root_command = Command::from_clap_recursive(&root, BTreeMap::new()).unwrap();
    let CommandData::Node(root_child_commands) = root_command.data else {
        bail!("Root command data is not a node");
    };

    let local_command = root_child_commands
        .get("local")
        .context("local command not found")?;

    // Create output directory
    fs::create_dir_all(OUTPUT_DIR).context("Failed to create output directory")?;

    // Generate docs recursively for the local command and all its subcommands
    generate_docs_recursive(local_command, "atlas local")?;

    println!("Documentation generated in {}/", OUTPUT_DIR);
    Ok(())
}

/// Recursively generates documentation for a command and all its subcommands.
fn generate_docs_recursive(command: &Command, full_command: &str) -> Result<()> {
    // Generate docs for this command
    let docs = command.generate_docs(full_command);

    // Write to file
    let filename = format!("{}/{}.txt", OUTPUT_DIR, full_command.replace(' ', "-"));
    fs::write(&filename, &docs).with_context(|| format!("Failed to write {}", filename))?;
    println!("Generated: {}", filename);

    // Recursively generate docs for subcommands
    if let CommandData::Node(subcommands) = &command.data {
        for (name, subcmd) in subcommands {
            let child_full_command = format!("{} {}", full_command, name);
            generate_docs_recursive(subcmd, &child_full_command)?;
        }
    }

    Ok(())
}

#[derive(Clone, Debug)]
pub struct Command {
    pub description: String,
    pub flags: BTreeMap<String, Flag>,
    pub inherited_flags: BTreeMap<String, Flag>,
    pub data: CommandData,
}

impl Command {
    fn from_clap_recursive(
        command: &clap::Command,
        inherited_flags: BTreeMap<String, Flag>,
    ) -> Result<Command> {
        let description = command
            .get_long_about()
            .or_else(|| command.get_about())
            .map(|s| s.to_string())
            .unwrap_or_default();

        let mut flags = BTreeMap::new();
        for opt in command.get_opts() {
            if let Some(flag) = maybe_try_from_arg(opt)? {
                flags.insert(opt.get_id().to_string(), flag);
            }
        }

        let mut subcommands = BTreeMap::new();
        for subcommand in command.get_subcommands() {
            if subcommand.is_hide_set() {
                continue;
            }

            subcommands.insert(
                subcommand.get_name().to_string(),
                Self::from_clap_recursive(
                    subcommand,
                    inherited_flags
                        .iter()
                        .chain(flags.iter())
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                )?,
            );
        }

        let data = match subcommands.len() {
            0 => CommandData::Leaf(Leaf {
                args: command
                    .get_arguments()
                    .map(Arg::try_from)
                    .collect::<Result<Vec<_>>>()
                    .context("converting clap::Args to Args")?,
            }),
            _ => CommandData::Node(subcommands),
        };

        Ok(Command {
            description,
            flags,
            inherited_flags,
            data,
        })
    }

    /// Generates snooty documentation for this command.
    ///
    /// # Arguments
    /// * `full_command` - The full command path (e.g., "atlas deployments start")
    pub fn generate_docs(&self, full_command: &str) -> String {
        let mut output = String::new();
        let reference = full_command.replace(' ', "-");

        // Reference anchor
        writeln!(output, ".. _{}:\n", reference).unwrap();

        // Title with equals signs above and below
        let equals = "=".repeat(full_command.len());
        writeln!(output, "{}", equals).unwrap();
        writeln!(output, "{}", full_command).unwrap();
        writeln!(output, "{}", equals).unwrap();

        // Table of contents
        output.push_str(TOC);

        // Description (use first line only for short description)
        writeln!(output, "\n{}\n", self.description).unwrap();

        match &self.data {
            CommandData::Leaf(leaf) => {
                self.write_leaf_docs(&mut output, full_command, leaf);
            }
            CommandData::Node(subcommands) => {
                self.write_node_docs(&mut output, full_command, subcommands);
            }
        }

        output
    }

    fn write_leaf_docs(&self, output: &mut String, full_command: &str, leaf: &Leaf) {
        // Syntax section
        output.push_str(SYNTAX_HEADER);
        let usage_line = self.build_usage_line(full_command, leaf);
        writeln!(output, "\n   {}\n", usage_line).unwrap();
        output.push_str(".. Code end marker, please don't delete this comment\n\n");

        // Arguments section (only positional args, not flags)
        let positional_args: Vec<_> = leaf
            .args
            .iter()
            .filter(|arg| {
                !self.flags.contains_key(&arg.name)
                    && !self.inherited_flags.contains_key(&arg.name)
                    && arg.flag_type.is_some()
            })
            .collect();

        if !positional_args.is_empty() {
            self.write_args_section(output, &positional_args);
        }

        // Options section (always show for help flag)
        self.write_flags_section(output, "Options", &self.flags, Some(full_command));

        // Inherited Options section
        if !self.inherited_flags.is_empty() {
            self.write_flags_section(output, "Inherited Options", &self.inherited_flags, None);
        }
    }

    fn write_node_docs(
        &self,
        output: &mut String,
        full_command: &str,
        subcommands: &BTreeMap<String, Command>,
    ) {
        // Options section (always show for help flag)
        self.write_flags_section(output, "Options", &self.flags, Some(full_command));

        // Inherited Options section
        if !self.inherited_flags.is_empty() {
            self.write_flags_section(output, "Inherited Options", &self.inherited_flags, None);
        }

        // Related Commands section
        if !subcommands.is_empty() {
            output.push_str("Related Commands\n");
            output.push_str("----------------\n\n");

            for (name, cmd) in subcommands {
                let child_command = format!("{} {}", full_command, name);
                let child_ref = child_command.replace(' ', "-");
                // Use only the first line of description for related commands
                let short_desc = cmd.description.lines().next().unwrap_or("");
                writeln!(output, "* :ref:`{}` - {}", child_ref, short_desc).unwrap();
            }
            output.push('\n');

            // toctree section
            output.push_str(TOCTREE_HEADER);
            output.push('\n');

            for name in subcommands.keys() {
                let child_command = format!("{} {}", full_command, name);
                let child_ref = child_command.replace(' ', "-");
                writeln!(output, "   {} </command/{}>", name, child_ref).unwrap();
            }
        }
    }

    fn build_usage_line(&self, full_command: &str, leaf: &Leaf) -> String {
        let mut parts = vec![full_command.to_string()];

        // Add positional arguments
        for arg in &leaf.args {
            // Skip flags (they're shown in [options])
            if self.flags.contains_key(&arg.name) || self.inherited_flags.contains_key(&arg.name) {
                continue;
            }
            // Skip args without a type (likely special actions like help)
            if arg.flag_type.is_none() {
                continue;
            }

            if arg.required {
                parts.push(format!("<{}>", arg.name));
            } else {
                parts.push(format!("[{}]", arg.name));
            }
        }

        parts.push("[options]".to_string());
        parts.join(" ")
    }

    fn write_flags_section(
        &self,
        output: &mut String,
        title: &str,
        flags: &BTreeMap<String, Flag>,
        full_command: Option<&str>,
    ) {
        writeln!(output, "{}", title).unwrap();
        writeln!(output, "{}\n", "-".repeat(title.len())).unwrap();

        output.push_str(".. list-table::\n");
        output.push_str("   :header-rows: 1\n");
        output.push_str("   :widths: 20 10 10 60\n\n");

        output.push_str("   * - Name\n");
        output.push_str("     - Type\n");
        output.push_str("     - Required\n");
        output.push_str("     - Description\n");

        // Add hardcoded help flag for Options section
        if let Some(cmd) = full_command {
            let last_command = cmd.split_whitespace().last().unwrap_or(cmd);
            output.push_str("   * - -h, --help\n");
            output.push_str("     - \n");
            output.push_str("     - false\n");
            writeln!(output, "     - help for {}", last_command).unwrap();
        }

        for flag in flags.values() {
            let type_str = flag.flag_type.as_ref().map(|t| t.type_name()).unwrap_or("");
            let required_str = if flag.required { "true" } else { "false" };
            let desc = flag.description.as_deref().unwrap_or("");

            // Format flag name with short and/or long form
            let flag_name = match (&flag.short, &flag.long) {
                (Some(s), Some(l)) => format!("-{}, --{}", s, l),
                (Some(s), None) => format!("-{}", s),
                (None, Some(l)) => format!("--{}", l),
                (None, None) => flag.name.clone(),
            };

            writeln!(output, "   * - {}", flag_name).unwrap();
            writeln!(output, "     - {}", type_str).unwrap();
            writeln!(output, "     - {}", required_str).unwrap();
            writeln!(output, "     - {}", desc).unwrap();
        }

        output.push('\n');
    }

    fn write_args_section(&self, output: &mut String, args: &[&Arg]) {
        output.push_str("Arguments\n");
        output.push_str("---------\n\n");

        output.push_str(".. list-table::\n");
        output.push_str("   :header-rows: 1\n");
        output.push_str("   :widths: 20 10 10 60\n\n");

        output.push_str("   * - Name\n");
        output.push_str("     - Type\n");
        output.push_str("     - Required\n");
        output.push_str("     - Description\n");

        for arg in args {
            let type_str = arg
                .flag_type
                .as_ref()
                .map(|t| t.type_name())
                .unwrap_or("string");
            let required_str = if arg.required { "true" } else { "false" };
            let desc = arg.description.as_deref().unwrap_or("");

            writeln!(output, "   * - {}", arg.name).unwrap();
            writeln!(output, "     - {}", type_str).unwrap();
            writeln!(output, "     - {}", required_str).unwrap();
            writeln!(output, "     - {}", desc).unwrap();
        }

        output.push('\n');
    }
}

#[derive(Clone, Debug)]
pub enum CommandData {
    Leaf(Leaf),
    Node(BTreeMap<String, Command>),
}

#[derive(Clone, Debug)]
pub struct Leaf {
    pub args: Vec<Arg>,
}

#[derive(Clone, Debug)]
pub struct Arg {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
    pub flag_type: Option<FlagType>,
}

impl TryFrom<&clap::Arg> for Arg {
    type Error = anyhow::Error;

    fn try_from(value: &clap::Arg) -> Result<Self, Self::Error> {
        Ok(Arg {
            name: value.get_id().to_string(),
            description: value
                .get_long_help()
                .or_else(|| value.get_help())
                .map(|s| s.to_string()),
            required: value.is_required_set(),
            flag_type: try_get_flag_type(value.get_action(), value.get_value_parser())
                .context("Failed to get argument type")?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Flag {
    pub name: String,
    pub short: Option<char>,
    pub long: Option<String>,
    pub flag_type: Option<FlagType>,
    pub required: bool,
    pub description: Option<String>,
}

fn maybe_try_from_arg(value: &clap::Arg) -> Result<Option<Flag>, anyhow::Error> {
    if value.is_hide_set() {
        return Ok(None);
    }

    Ok(Some(Flag {
        name: value.get_id().to_string(),
        short: value.get_short(),
        long: value.get_long().map(|s| s.to_string()),
        flag_type: try_get_flag_type(value.get_action(), value.get_value_parser())
            .context("Failed to get flag type")?,
        required: value.is_required_set(),
        description: value
            .get_long_help()
            .or_else(|| value.get_help())
            .map(|s| s.to_string()),
    }))
}

#[derive(Clone, Debug)]
pub enum FlagType {
    Boolean,
    Duration,
    Enum(Vec<String>),
    MongoDBVersion,
    Number,
    Path,
    String,
}

impl FlagType {
    /// Returns the type name to display in documentation.
    /// Boolean flags show an empty type (they're toggle switches).
    fn type_name(&self) -> &'static str {
        match self {
            FlagType::Boolean => "",
            FlagType::Duration => "string",
            FlagType::Enum(_) => "string",
            FlagType::MongoDBVersion => "string",
            FlagType::Number => "int",
            FlagType::Path => "string",
            FlagType::String => "string",
        }
    }
}

fn try_get_flag_type(
    action: &clap::ArgAction,
    value_parser: &ValueParser,
) -> Result<Option<FlagType>> {
    Ok(match action {
        clap::ArgAction::Set => match value_parser.type_id() {
            t if t == TypeId::of::<bool>() => Some(FlagType::Boolean),
            t if t == TypeId::of::<u16>() => Some(FlagType::Number),
            t if t == TypeId::of::<u32>() => Some(FlagType::Number),
            t if t == TypeId::of::<u64>() => Some(FlagType::Number),
            t if t == TypeId::of::<i16>() => Some(FlagType::Number),
            t if t == TypeId::of::<i32>() => Some(FlagType::Number),
            t if t == TypeId::of::<i64>() => Some(FlagType::Number),
            t if t == TypeId::of::<f32>() => Some(FlagType::Number),
            t if t == TypeId::of::<f64>() => Some(FlagType::Number),
            t if t == TypeId::of::<MongoDBVersion>() => Some(FlagType::MongoDBVersion),
            t if t == TypeId::of::<PathBuf>() => Some(FlagType::Path),
            t if t == TypeId::of::<String>() => Some(FlagType::String),
            t if t == TypeId::of::<Duration>() => Some(FlagType::Duration),
            _ => {
                if let Some(possible_values) = value_parser.possible_values() {
                    Some(FlagType::Enum(
                        possible_values.map(|v| v.get_name().to_string()).collect(),
                    ))
                } else {
                    Some(FlagType::String)
                }
            }
        },
        clap::ArgAction::Append => {
            bail!("Append action is not supported, we don't support lists of values")
        }
        clap::ArgAction::SetTrue | clap::ArgAction::SetFalse => Some(FlagType::Boolean),
        clap::ArgAction::Count => {
            bail!("Count action is not supported, we don't support counting flags")
        }
        clap::ArgAction::Help
        | clap::ArgAction::HelpShort
        | clap::ArgAction::HelpLong
        | clap::ArgAction::Version
        | _ => None,
    })
}
