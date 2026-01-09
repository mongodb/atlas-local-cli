use std::time::Duration;

use anyhow::{Context, Result};
use console::{Emoji, Style};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use super::{
    Interaction, MultiStepSpinner, MultiStepSpinnerInteraction, MultiStepSpinnerOutcome,
    MultiStepSpinnerStep, SpinnerHandle, SpinnerInteraction,
};

impl SpinnerInteraction for Interaction {
    fn start_spinner(&self, message: String) -> Result<SpinnerHandle> {
        let progress_bar = ProgressBar::new_spinner();
        progress_bar.set_message(message);
        progress_bar.enable_steady_tick(Duration::from_millis(80));

        Ok(SpinnerHandle::new(Box::new(move || {
            progress_bar.finish_and_clear();
        })))
    }
}

impl MultiStepSpinnerInteraction for Interaction {
    fn start_multi_step_spinner(
        &self,
        steps: Vec<MultiStepSpinnerStep>,
    ) -> Result<Box<dyn MultiStepSpinner + Send + Sync>> {
        // Safe to unwrap because we're using a static string for the template
        let progress_style = ProgressStyle::with_template("{prefix} {spinner} {msg}")
            .expect("failed to create progress style");

        let m = MultiProgress::new();
        let number_of_steps = steps.len();

        let step_progress_bars = steps
            .into_iter()
            .enumerate()
            .map(|(index, step)| {
                // Create a new progress bar for each step
                let progress_bar = m.add(ProgressBar::new_spinner());

                // Set the style and enable the steady tick
                progress_bar.set_style(progress_style.clone());
                progress_bar.enable_steady_tick(Duration::from_millis(80));

                // Set the prefix to the styled step number
                progress_bar.set_prefix(prefix(index + 1, number_of_steps, None));

                // Set the message to the step message
                progress_bar.set_message(step.message);

                // Return the progress bar
                progress_bar
            })
            .collect();

        Ok(Box::new(IndicatifMultiStepSpinner { step_progress_bars }))
    }
}

fn prefix(step: usize, number_of_steps: usize, outcome: Option<MultiStepSpinnerOutcome>) -> String {
    let text = match outcome {
        Some(MultiStepSpinnerOutcome::Success) => {
            format!("[{step}/{number_of_steps}] [{}]", Emoji("✅", "✓"))
        }
        Some(MultiStepSpinnerOutcome::Failure) => {
            format!("[{step}/{number_of_steps}] [{}]", Emoji("❌", "✗"))
        }
        Some(MultiStepSpinnerOutcome::Skipped) => {
            format!("[{step}/{number_of_steps}] [{}]", Emoji("⏭️", "-"))
        }
        None => format!("[{step}/{number_of_steps}] [{}]", Emoji("⏳", " ")),
    };

    let mut style = Style::new();

    match outcome {
        Some(MultiStepSpinnerOutcome::Success) | None => {}
        Some(MultiStepSpinnerOutcome::Failure) => style = style.red(),
        Some(MultiStepSpinnerOutcome::Skipped) => style = style.dim(),
    }

    style.apply_to(&text).to_string()
}

pub struct IndicatifMultiStepSpinner {
    step_progress_bars: Vec<ProgressBar>,
}

impl MultiStepSpinner for IndicatifMultiStepSpinner {
    fn set_step_outcome(&mut self, step: usize, outcome: MultiStepSpinnerOutcome) -> Result<()> {
        let number_of_steps = self.step_progress_bars.len();
        let step_progress_bar = self
            .step_progress_bars
            .get_mut(step)
            .with_context(|| format!("step {step} not found"))?;

        step_progress_bar.set_prefix(prefix(step + 1, number_of_steps, Some(outcome)));
        step_progress_bar.finish();

        Ok(())
    }
}
