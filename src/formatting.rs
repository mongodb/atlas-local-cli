use std::fmt::Display;

use anyhow::{Context, Result};
use clap::ValueEnum;
use serde::Serialize;

/// Format of the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum)]
pub enum Format {
    Text,
    Json,
}

// trait which represents something that can be formatted as text or json
// The main use of this trait is to format the output of a command.
pub trait Formattable {
    /// Format the output of the object as text or json.
    fn format(&self, format: Format) -> Result<String>;
}

impl<T> Formattable for T
where
    T: Display + Serialize,
{
    fn format(&self, format: Format) -> Result<String> {
        Ok(match format {
            Format::Text => self.to_string(),
            Format::Json => serde_json::to_string(self).context("serializing to json")?,
        })
    }
}
