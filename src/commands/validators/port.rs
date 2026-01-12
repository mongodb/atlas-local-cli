//! Validator for port numbers.

use anyhow::Result;

use crate::interaction::{InputValidator, InputValidatorResult};

/// Validator for port numbers.
///
/// Validates that the input is either empty, "auto-assign", or a valid port
/// number between 1 and 65535.
#[derive(Clone)]
pub struct PortValidator;

const PORT_ERROR_MESSAGE: &str =
    "Port must be a number between 1 and 65535, leave empty to auto-assign";

impl InputValidator for PortValidator {
    fn validate(&self, input: &str) -> Result<InputValidatorResult> {
        let invalid_port_result = || {
            Ok(InputValidatorResult::Invalid(
                PORT_ERROR_MESSAGE.to_string(),
            ))
        };

        if input.is_empty() || input == "auto-assign" {
            return Ok(InputValidatorResult::Valid);
        }

        match input.parse::<u16>() {
            Ok(port) => {
                if port == 0 {
                    return invalid_port_result();
                }
                Ok(InputValidatorResult::Valid)
            }
            Err(_) => invalid_port_result(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_validator() {
        let validator = PortValidator;

        // Test valid ports
        assert!(matches!(
            validator.validate("").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("auto-assign").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("27017").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("1").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("65535").unwrap(),
            InputValidatorResult::Valid
        ));

        // Test invalid ports
        assert!(matches!(
            validator.validate("0").unwrap(),
            InputValidatorResult::Invalid(_)
        ));
        assert!(matches!(
            validator.validate("65536").unwrap(),
            InputValidatorResult::Invalid(_)
        ));
        assert!(matches!(
            validator.validate("invalid").unwrap(),
            InputValidatorResult::Invalid(_)
        ));
    }
}
