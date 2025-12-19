use std::time::Duration;

use anyhow::Result;
use indicatif::ProgressBar;

use super::{Interaction, SpinnerHandle, SpinnerInteraction};

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
