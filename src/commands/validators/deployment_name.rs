//! Validator for deployment names.

use anyhow::Result;

use crate::interaction::{InputValidator, InputValidatorResult};

/// Validator for deployment names.
///
/// Currently accepts any non-empty string as valid.
#[derive(Clone)]
pub struct DeploymentNameValidator;

impl InputValidator for DeploymentNameValidator {
    fn validate(&self, _: &str) -> Result<InputValidatorResult> {
        Ok(InputValidatorResult::Valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_name_validator() {
        let validator = DeploymentNameValidator;
        assert!(matches!(
            validator.validate("test-deployment").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("").unwrap(),
            InputValidatorResult::Valid
        ));
    }
}
