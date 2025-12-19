use anyhow::Result;
use typed_builder::TypedBuilder;

mod input;
mod spinner;

#[derive(Debug, Default, Clone)]
pub struct Interaction;

impl Interaction {
    pub fn new() -> Self {
        Default::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, TypedBuilder)]
pub struct ConfirmationPromptOptions<'a> {
    message: &'a str,
    #[builder(default, setter(strip_option))]
    default: Option<bool>,
    #[builder(default, setter(strip_option))]
    pre_confirmation_help_text: Option<&'a str>,
    #[builder(default, setter(strip_option))]
    post_confirmation_help_text: Option<&'a str>,
}

pub enum ConfirmationPromptResult {
    Yes,
    No,
    Canceled,
}

pub trait ConfirmationPrompt {
    fn confirm<'a>(
        &self,
        options: ConfirmationPromptOptions<'a>,
    ) -> Result<ConfirmationPromptResult>;
}

pub struct SpinnerHandle {
    stop_spinner: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl SpinnerHandle {
    pub fn new(stop_spinner: Box<dyn FnOnce() + Send + Sync>) -> Self {
        Self {
            stop_spinner: Some(stop_spinner),
        }
    }
}

impl Drop for SpinnerHandle {
    fn drop(&mut self) {
        if let Some(stop_spinner) = self.stop_spinner.take() {
            stop_spinner();
        }
    }
}

pub trait SpinnerInteraction {
    fn start_spinner(&self, message: String) -> Result<SpinnerHandle>;
}

#[cfg(test)]
pub mod mocks {
    use super::*;
    use mockall::mock;

    mock! {
        pub Interaction {}

        impl ConfirmationPrompt for Interaction {
            fn confirm<'a>(&self, options: ConfirmationPromptOptions<'a>) -> Result<ConfirmationPromptResult>;
        }

        impl SpinnerInteraction for Interaction {
            fn start_spinner(&self, message: String) -> Result<SpinnerHandle>;
        }
    }
}
