use anyhow::Result;
use inquire::{Confirm, InquireError};

use crate::interaction::ConfirmationPromptResult;

use super::{ConfirmationPrompt, ConfirmationPromptOptions, Interaction};

impl ConfirmationPrompt for Interaction {
    fn confirm(&self, options: ConfirmationPromptOptions) -> Result<ConfirmationPromptResult> {
        let mut prompt = Confirm::new(options.message);
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
