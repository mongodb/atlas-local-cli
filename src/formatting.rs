//! This module contains the formatting logic for the application.
//!
//! The main entry point is the [`Formattable`] trait which provides a method to format an object as text or json.
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

/// Trait for types that can be formatted as text or JSON.
///
/// The main use of this trait is to format the output of commands.
/// Types implementing both [`Display`] and [`Serialize`] automatically get a default implementation.
pub trait Formattable {
    /// Format the output of the object as text or json.
    fn format(&self, format: Format) -> Result<String>;
}

/// Implement [`Formattable`] for any type that implements [`Display`] and [`Serialize`].
///
/// When the text is requested, the object is converted to a string using the [`Display`] trait.
/// When the JSON is requested, the object is serialized to a JSON string using the [`Serialize`] trait.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct TestStruct {
        name: String,
        value: i32,
    }

    impl Display for TestStruct {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}: {}", self.name, self.value)
        }
    }

    #[test]
    fn test_format_text() {
        let test = TestStruct {
            name: "test".to_string(),
            value: 42,
        };
        let result = test.format(Format::Text).unwrap();
        assert_eq!(result, "test: 42");
    }

    #[test]
    fn test_format_json() {
        let test = TestStruct {
            name: "test".to_string(),
            value: 42,
        };
        let result = test.format(Format::Json).unwrap();
        assert_eq!(result, r#"{"name":"test","value":42}"#);
    }
}
