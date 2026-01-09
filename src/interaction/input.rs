use anyhow::Result;
use console::style;
use inquire::{Confirm, InquireError, Select, Text, validator::StringValidator};

use crate::interaction::{
    InputPrompt, InputPromptOptions, InputPromptResult, InputPromptValidator, InputValidatorResult,
};

use super::{
    ConfirmationPrompt, ConfirmationPromptOptions, ConfirmationPromptResult, Interaction,
    SelectPrompt, SelectPromptOptions, SelectPromptResult,
};

impl ConfirmationPrompt for Interaction {
    fn confirm(&self, options: ConfirmationPromptOptions) -> Result<ConfirmationPromptResult> {
        let mut prompt = Confirm::new(&options.message);
        if let Some(default) = options.default {
            prompt = prompt.with_default(default);
        }

        if let Some(help_text) = &options.post_confirmation_help_text {
            prompt = prompt.with_help_message(help_text);
        }

        if let Some(help_text) = &options.pre_confirmation_help_text {
            println!("{}", help_text);
        }

        match prompt.prompt() {
            Ok(true) => Ok(ConfirmationPromptResult::Yes),
            Ok(false) => Ok(ConfirmationPromptResult::No),
            Err(InquireError::OperationCanceled) => Ok(ConfirmationPromptResult::Canceled),
            Err(InquireError::OperationInterrupted) => Ok(ConfirmationPromptResult::Canceled),
            Err(err) => Err(anyhow::anyhow!("error prompting for confirmation: {}", err)),
        }
    }
}

// Implement the StringValidator trait for the InputPromptValidator
impl StringValidator for InputPromptValidator {
    fn validate(
        &self,
        input: &str,
    ) -> Result<inquire::validator::Validation, inquire::error::CustomUserError> {
        match self.0.validate(input) {
            Ok(InputValidatorResult::Valid) => Ok(inquire::validator::Validation::Valid),
            Ok(InputValidatorResult::Invalid(error)) => {
                Ok(inquire::validator::Validation::Invalid(
                    inquire::validator::ErrorMessage::Custom(error),
                ))
            }
            Err(e) => Err(inquire::error::CustomUserError::from(e)),
        }
    }
}

impl InputPrompt for Interaction {
    fn input(&self, options: InputPromptOptions) -> Result<InputPromptResult> {
        let mut prompt = Text::new(&options.message);

        // Set the default value if provided
        if let Some(default) = options.default.as_deref() {
            prompt = prompt.with_default(default);
        }

        // In case the input is finalized, show the default value as the final answer to the prompt
        if let Some(final_answer) = options.final_answer {
            let green_bracket = style(">").green();
            let message = options.message;
            let final_answer_styled = style(&final_answer).cyan();

            eprintln!("{green_bracket} {message} {final_answer_styled}");

            return Ok(InputPromptResult::Input(final_answer));
        }

        // Set the validator if provided
        if let Some(validator) = options.validator {
            prompt = prompt.with_validator(validator);
        }

        match prompt.prompt() {
            Ok(name) => Ok(InputPromptResult::Input(name)),
            Err(e) => match e {
                InquireError::OperationCanceled => Ok(InputPromptResult::Canceled),
                InquireError::OperationInterrupted => Ok(InputPromptResult::Canceled),
                _ => Err(anyhow::anyhow!("error prompting for input: {}", e)),
            },
        }
    }
}

impl SelectPrompt for Interaction {
    fn select(&self, options: SelectPromptOptions) -> Result<SelectPromptResult> {
        let select = Select::new(&options.message, options.options);

        match select.prompt() {
            Ok(selected) => Ok(SelectPromptResult::Selected(selected)),
            Err(InquireError::OperationCanceled) => Ok(SelectPromptResult::Canceled),
            Err(InquireError::OperationInterrupted) => Ok(SelectPromptResult::Canceled),
            Err(err) => Err(anyhow::anyhow!("error prompting for selection: {}", err)),
        }
    }
}
