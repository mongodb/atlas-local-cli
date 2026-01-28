use std::rc::Rc;

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
pub struct ConfirmationPromptOptions {
    message: String,
    #[builder(default, setter(strip_option))]
    placeholder: Option<String>,
    #[builder(default, setter(strip_option))]
    default: Option<bool>,
    #[builder(default, setter(strip_option))]
    pre_confirmation_help_text: Option<String>,
    #[builder(default, setter(strip_option))]
    post_confirmation_help_text: Option<String>,
}

pub enum ConfirmationPromptResult {
    Yes,
    No,
    Canceled,
}

pub trait ConfirmationPrompt {
    fn confirm(&self, options: ConfirmationPromptOptions) -> Result<ConfirmationPromptResult>;
}

#[derive(TypedBuilder)]
pub struct InputPromptOptions {
    pub message: String,
    #[builder(default, setter(strip_option(fallback = default_opt)))]
    pub default: Option<String>,
    #[builder(default, setter(strip_option))]
    pub validator: Option<InputPromptValidator>,
    // When this is set, the input prompt will not be prompted for
    // The input will immediately be returned as the final answer as if the user had already provided the input
    #[builder(default)]
    pub final_answer: Option<String>,
}

#[derive(Clone)]
// We're using an Rc because the validator needs to be cloneable, this is the most elegant way to do this
pub struct InputPromptValidator(Rc<dyn InputValidator>);

impl InputPromptValidator {
    pub fn new(validator: impl InputValidator + 'static) -> Self {
        Self(Rc::new(validator))
    }
}

pub trait InputValidator {
    fn validate(&self, input: &str) -> Result<InputValidatorResult>;
}

pub enum InputValidatorResult {
    Valid,
    Invalid(String),
}

pub enum InputPromptResult {
    Input(String),
    Canceled,
}

pub trait InputPrompt {
    fn input(&self, options: InputPromptOptions) -> Result<InputPromptResult>;

    /// Prompts the user for input if the field is `None`, otherwise returns the existing value.
    ///
    /// Returns an error if the user cancels the prompt.
    fn prompt_if_none(&self, field: Option<&str>, prompt: &str) -> Result<String> {
        match field {
            Some(value) => Ok(value.to_string()),
            None => match self.input(
                InputPromptOptions::builder()
                    .message(prompt.to_string())
                    .build(),
            )? {
                InputPromptResult::Input(value) => Ok(value),
                InputPromptResult::Canceled => Err(anyhow::anyhow!("user canceled the prompt")),
            },
        }
    }
}

#[derive(Debug, PartialEq, Eq, TypedBuilder)]
pub struct SelectPromptOptions {
    #[builder(setter(transform = |s: impl Into<String>| s.into()))]
    message: String,
    #[builder(setter(transform = |items: impl IntoIterator<Item = impl Into<String>>| {
        items.into_iter().map(|s| s.into()).collect()
    }))]
    options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectPromptResult {
    Selected(String),
    Canceled,
}

pub trait SelectPrompt {
    fn select(&self, options: SelectPromptOptions) -> Result<SelectPromptResult>;
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

pub trait MultiStepSpinnerInteraction {
    fn start_multi_step_spinner(
        &self,
        steps: Vec<MultiStepSpinnerStep>,
    ) -> Result<Box<dyn MultiStepSpinner + Send + Sync>>;
}

pub struct MultiStepSpinnerStep {
    message: String,
}

impl MultiStepSpinnerStep {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub trait MultiStepSpinner {
    fn set_step_outcome(&mut self, step: usize, outcome: MultiStepSpinnerOutcome) -> Result<()>;
}

pub enum MultiStepSpinnerOutcome {
    Success,
    Skipped,
    Failure,
}

#[cfg(test)]
pub mod mocks {
    use super::*;
    use mockall::mock;

    mock! {
        pub Interaction {}

        impl ConfirmationPrompt for Interaction {
            fn confirm(&self, options: ConfirmationPromptOptions) -> Result<ConfirmationPromptResult>;
        }

        impl SpinnerInteraction for Interaction {
            fn start_spinner(&self, message: String) -> Result<SpinnerHandle>;
        }

        impl InputPrompt for Interaction {
            fn input(&self, options: InputPromptOptions) -> Result<InputPromptResult>;
        }

        impl SelectPrompt for Interaction {
            fn select(&self, options: SelectPromptOptions) -> Result<SelectPromptResult>;
        }

        impl MultiStepSpinnerInteraction for Interaction {
            fn start_multi_step_spinner(&self, steps: Vec<MultiStepSpinnerStep>) -> Result<Box<dyn MultiStepSpinner + Send + Sync>>;
        }
    }
}
