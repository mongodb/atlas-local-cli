//! Validator for image tag strings.

use anyhow::Result;
use atlas_local::models::{
    ImageTag, MongoDBVersion, MongoDBVersionMajor, MongoDBVersionMajorMinor,
    MongoDBVersionMajorMinorPatch,
};

use crate::interaction::{InputValidator, InputValidatorResult};

/// Validator for image tag strings.
///
/// Validates that the input is either 'preview', 'latest', a semver version
/// (major, major.minor, or major.minor.patch format), or a semver+timestamp.
/// Also ensures that numeric versions have a major version of at least 7.
#[derive(Clone)]
pub struct ImageTagValidator;

impl InputValidator for ImageTagValidator {
    fn validate(&self, input: &str) -> Result<InputValidatorResult> {
        match ImageTag::try_from(input) {
            Ok(tag) => match tag {
                ImageTag::Preview | ImageTag::Latest => Ok(InputValidatorResult::Valid),
                ImageTag::Semver(v) | ImageTag::SemverTimestamp(v, _) => match v {
                    MongoDBVersion::Major(MongoDBVersionMajor { major })
                    | MongoDBVersion::MajorMinor(MongoDBVersionMajorMinor { major, .. })
                    | MongoDBVersion::MajorMinorPatch(MongoDBVersionMajorMinorPatch {
                        major,
                        ..
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
            },
            Err(e) => Ok(InputValidatorResult::Invalid(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_tag_validator() {
        let validator = ImageTagValidator;

        // Test valid versions
        assert!(matches!(
            validator.validate("latest").unwrap(),
            InputValidatorResult::Valid
        ));
        assert!(matches!(
            validator.validate("preview").unwrap(),
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
        assert!(matches!(
            validator.validate("8.2.4-20260217T084055Z").unwrap(),
            InputValidatorResult::Valid
        ));

        // Test invalid versions
        assert!(matches!(
            validator.validate("6").unwrap(),
            InputValidatorResult::Invalid(_)
        ));
        assert!(matches!(
            validator.validate("6.0.0-20260217T084055Z").unwrap(),
            InputValidatorResult::Invalid(_)
        ));
        assert!(matches!(
            validator.validate("invalid").unwrap(),
            InputValidatorResult::Invalid(_)
        ));
    }
}
