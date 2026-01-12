//! Validator for MongoDB version strings.

use anyhow::Result;
use atlas_local::models::{
    MongoDBVersion, MongoDBVersionMajor, MongoDBVersionMajorMinor, MongoDBVersionMajorMinorPatch,
};

use crate::interaction::{InputValidator, InputValidatorResult};

/// Validator for MongoDB version strings.
///
/// Validates that the version string is either "latest" or a valid version
/// number (major, major.minor, or major.minor.patch format).
/// Also ensures that the major version is at least 7.
#[derive(Clone)]
pub struct MdbVersionValidator;

impl InputValidator for MdbVersionValidator {
    fn validate(&self, input: &str) -> Result<InputValidatorResult> {
        match MongoDBVersion::try_from(input) {
            Ok(v) => match v {
                MongoDBVersion::Latest => Ok(InputValidatorResult::Valid),
                MongoDBVersion::Major(MongoDBVersionMajor { major })
                | MongoDBVersion::MajorMinor(MongoDBVersionMajorMinor { major, .. })
                | MongoDBVersion::MajorMinorPatch(MongoDBVersionMajorMinorPatch {
                    major, ..
                }) => {
                    if major < 7 {
                        Ok(InputValidatorResult::Invalid(
                            "The lowest supported MongoDB version is 7".to_string(),
                        ))
                    } else {
                        Ok(InputValidatorResult::Valid)
                    }
                }
            },
            Err(e) => Ok(InputValidatorResult::Invalid(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mdb_version_validator() {
        let validator = MdbVersionValidator;

        // Test valid versions
        assert!(matches!(
            validator.validate("latest").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("7").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("8").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("8.2").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("8.2.2").unwrap(),
            InputValidatorResult::Valid
        ));

        // Test invalid versions
        assert!(matches!(
            validator.validate("6").unwrap(),
            InputValidatorResult::Invalid(_)
        ));
        assert!(matches!(
            validator.validate("invalid").unwrap(),
            InputValidatorResult::Invalid(_)
        ));
    }
}
