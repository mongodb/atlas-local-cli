//! Validator for yes/no input and helper function.

use anyhow::Result;

use crate::interaction::{InputValidator, InputValidatorResult};

/// Validator for yes/no input.
///
/// Validates that the input is a valid yes/no response (y, yes, n, no, or empty).
#[derive(Clone)]
pub struct YesNoValidator;

impl InputValidator for YesNoValidator {
    fn validate(&self, input: &str) -> Result<InputValidatorResult> {
        match yes_no_to_bool(input, false) {
            Ok(_) => Ok(InputValidatorResult::Valid),
            Err(e) => Ok(InputValidatorResult::Invalid(e)),
        }
    }
}

/// Convert a yes/no string to a boolean value.
///
/// Accepts "y", "yes", "n", "no" (case-insensitive) or empty string (uses default).
///
/// # Arguments
///
/// * `input` - The input string to convert
/// * `default` - The default value to use if input is empty
///
/// # Returns
///
/// Returns `Ok(bool)` if the input is valid, or `Err(String)` with an error message.
pub fn yes_no_to_bool(input: &str, default: bool) -> Result<bool, String> {
    match input.to_lowercase().as_str() {
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        "" => Ok(default),
        _ => Err(format!("Invalid input '{input}', please enter y or n")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yes_no_validator() {
        let validator = YesNoValidator;

        // Test valid inputs
        assert!(matches!(
            validator.validate("y").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("Y").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("yes").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("YES").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("n").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("N").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("no").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("NO").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("").unwrap(),
            InputValidatorResult::Valid
        ));

        // Test invalid inputs
        assert!(matches!(
            validator.validate("maybe").unwrap(),
            InputValidatorResult::Invalid(_)
        ));
        assert!(matches!(
            validator.validate("1").unwrap(),
            InputValidatorResult::Invalid(_)
        ));
    }

    #[test]
    fn test_yes_no_to_bool() {
        // Test valid inputs
        assert_eq!(yes_no_to_bool("y", false).unwrap(), true);
        assert_eq!(yes_no_to_bool("Y", false).unwrap(), true);
        assert_eq!(yes_no_to_bool("yes", false).unwrap(), true);
        assert_eq!(yes_no_to_bool("YES", false).unwrap(), true);
        assert_eq!(yes_no_to_bool("n", true).unwrap(), false);
        assert_eq!(yes_no_to_bool("N", true).unwrap(), false);
        assert_eq!(yes_no_to_bool("no", true).unwrap(), false);
        assert_eq!(yes_no_to_bool("NO", true).unwrap(), false);
        assert_eq!(yes_no_to_bool("", true).unwrap(), true);
        assert_eq!(yes_no_to_bool("", false).unwrap(), false);

        // Test invalid inputs
        assert!(yes_no_to_bool("maybe", false).is_err());
        assert!(yes_no_to_bool("1", false).is_err());
    }
}
