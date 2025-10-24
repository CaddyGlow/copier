use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::collections::HashMap;
use std::io::IsTerminal;
use std::sync::Arc;
use std::time::Duration;

use super::jsonrpc::ProgressState;

/// Manages progress display for the analyze command
pub struct ProgressDisplay {
    multi: MultiProgress,
    enabled: bool,
}

impl ProgressDisplay {
    /// Create a new progress display
    /// Auto-detects whether stderr is a terminal and respects verbosity
    pub fn new(verbose: u8) -> Self {
        let is_terminal = std::io::stderr().is_terminal();
        // Only show progress when stderr is a terminal (not redirected)
        // Progress bars work fine alongside debug/trace output
        let enabled = is_terminal;
        let _ = verbose; // Keep parameter for potential future use

        let multi = MultiProgress::new();
        if !enabled {
            // Disable drawing to avoid any output
            multi.set_draw_target(ProgressDrawTarget::hidden());
        }

        Self { multi, enabled }
    }

    /// Check if progress display is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Create a spinner with a message
    pub fn spinner(&self, msg: impl Into<String>) -> ProgressBar {
        if !self.enabled {
            return ProgressBar::hidden();
        }

        let pb = self.multi.add(ProgressBar::new_spinner());
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message(msg.into());
        pb.enable_steady_tick(Duration::from_millis(80));
        pb
    }

    /// Create a progress bar for a known count of items
    /// The msg should be just the step number like "[2/4]"
    pub fn progress_bar(&self, len: u64, step: impl Into<String>) -> ProgressBar {
        if !self.enabled {
            return ProgressBar::hidden();
        }

        let pb = self.multi.add(ProgressBar::new(len));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{prefix} {spinner:.cyan} {wide_msg}\n{bar:40.cyan/blue} {pos}/{len}")
                .unwrap()
                .progress_chars("█▓▒░")
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.enable_steady_tick(Duration::from_millis(80));
        pb.set_prefix(step.into());
        pb
    }

    /// Create a progress bar that shows percentage
    pub fn progress_bar_with_percentage(
        &self,
        len: u64,
        prefix: impl Into<String>,
    ) -> ProgressBar {
        if !self.enabled {
            return ProgressBar::hidden();
        }

        let pb = self.multi.add(ProgressBar::new(len));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{prefix:.bold} {bar:40.cyan/blue} {percent}% {msg}")
                .unwrap()
                .progress_chars("█▓▒░"),
        );
        pb.set_prefix(prefix.into());
        pb
    }

    /// Manager for LSP progress notifications
    pub fn lsp_progress_manager(&self) -> LspProgressManager {
        LspProgressManager::new(self.multi.clone(), self.enabled)
    }
}

/// Manages multiple LSP progress bars based on progress notifications
pub struct LspProgressManager {
    multi: MultiProgress,
    enabled: bool,
    progress_bars: Arc<std::sync::Mutex<HashMap<String, ProgressBar>>>,
}

impl LspProgressManager {
    fn new(multi: MultiProgress, enabled: bool) -> Self {
        Self {
            multi,
            enabled,
            progress_bars: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Update progress bars based on current LSP progress states
    pub fn update(&self, progress_states: &HashMap<String, ProgressState>) {
        if !self.enabled {
            return;
        }

        let mut bars = self.progress_bars.lock().unwrap();

        for (token, state) in progress_states {
            match state {
                ProgressState::Begin { title, message } => {
                    // Create new progress bar for this token
                    if !bars.contains_key(token) {
                        let pb = self.multi.add(ProgressBar::new(100));
                        pb.set_style(
                            ProgressStyle::default_bar()
                                .template("{spinner:.green} {prefix:.bold.cyan} {bar:30.cyan/blue} {percent}% {msg}")
                                .unwrap()
                                .progress_chars("█▓▒░"),
                        );
                        pb.enable_steady_tick(Duration::from_millis(100));
                        pb.set_prefix(title.clone());
                        if let Some(msg) = message {
                            pb.set_message(msg.clone());
                        }
                        pb.set_position(0);
                        bars.insert(token.clone(), pb);
                    }
                }
                ProgressState::Report {
                    message,
                    percentage,
                } => {
                    // Update existing progress bar
                    if let Some(pb) = bars.get(token) {
                        if let Some(pct) = percentage {
                            pb.set_position(*pct as u64);
                        }
                        if let Some(msg) = message {
                            pb.set_message(msg.clone());
                        }
                    }
                }
                ProgressState::End { message } => {
                    // Finish and remove progress bar
                    if let Some(pb) = bars.remove(token) {
                        if let Some(msg) = message {
                            pb.finish_with_message(msg.clone());
                        } else {
                            pb.finish_and_clear();
                        }
                    }
                }
            }
        }

        // Clean up any bars that are no longer in the progress states
        bars.retain(|token, pb| {
            if !progress_states.contains_key(token) {
                pb.finish_and_clear();
                false
            } else {
                true
            }
        });
    }

    /// Check if there are any active progress bars
    pub fn has_active(&self) -> bool {
        if !self.enabled {
            return false;
        }
        !self.progress_bars.lock().unwrap().is_empty()
    }

    /// Clear all progress bars
    pub fn clear(&self) {
        if !self.enabled {
            return;
        }
        let mut bars = self.progress_bars.lock().unwrap();
        for (_, pb) in bars.drain() {
            pb.finish_and_clear();
        }
    }
}
