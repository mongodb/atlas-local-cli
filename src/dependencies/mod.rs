//! This module defines traits for external dependencies (such as Docker interactions) to make them
//! easier to mock and substitute in tests or other environments. By abstracting external services
//! behind traits, components can be decoupled and dependency-injected, improving testability and maintainability.
pub mod docker;
pub mod fs;
pub mod mongodb;

pub use docker::*;
pub use fs::*;
pub use mongodb::*;

#[cfg(test)]
pub mod mocks {
    pub use super::docker::mocks::*;
    pub use super::fs::mocks::*;
    pub use super::mongodb::mocks::*;
}
