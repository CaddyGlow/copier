use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use dialoguer::Confirm;
use tracing::{debug, info, warn};

use crate::config::{AppContext, UpdateConfig};
use crate::error::{QuickctxError, Result};

const UPDATE_CHECK_INTERVAL_DAYS: u64 = 7;
const REPO_OWNER: &str = "CaddyGlow";
const REPO_NAME: &str = "quickctx";

/// Run the update command to check for and install updates
pub fn run(_context: &AppContext, config: UpdateConfig) -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");

    info!("Current version: {}", current_version);
    info!("Checking for updates from GitHub releases...");

    let status = check_for_update()?;

    match status {
        UpdateStatus::NoUpdateAvailable => {
            println!(
                "You are already using the latest version ({})",
                current_version
            );
            Ok(())
        }
        UpdateStatus::UpdateAvailable {
            version,
            release_notes,
        } => {
            if config.check_only {
                println!("Update available: {} -> {}", current_version, version);
                if !release_notes.is_empty() {
                    println!("\nRelease notes:");
                    println!("{}", release_notes);
                }
                println!("\nRun 'quickctx update' to install the latest version");
                return Ok(());
            }

            let should_update = if config.yes {
                true
            } else {
                Confirm::new()
                    .with_prompt(format!(
                        "Update available: {} -> {}. Install now?",
                        current_version, version
                    ))
                    .default(true)
                    .interact()
                    .map_err(|e| {
                        QuickctxError::SelfUpdate(format!("failed to get user confirmation: {}", e))
                    })?
            };

            if should_update {
                install_update()?;
                println!("✓ Successfully updated to version {}", version);
            } else {
                println!("Update cancelled");
            }

            Ok(())
        }
    }
}

/// Check if an update is available and return the status
fn check_for_update() -> Result<UpdateStatus> {
    let current_version = env!("CARGO_PKG_VERSION");

    let releases = self_update::backends::github::ReleaseList::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .build()
        .map_err(|e| QuickctxError::SelfUpdate(format!("failed to configure release list: {}", e)))?
        .fetch()
        .map_err(|e| QuickctxError::SelfUpdate(format!("failed to fetch releases: {}", e)))?;

    if let Some(latest) = releases.first() {
        let latest_version = latest.version.trim_start_matches('v');

        if latest_version != current_version {
            debug!(
                "Update available: {} -> {}",
                current_version, latest_version
            );
            return Ok(UpdateStatus::UpdateAvailable {
                version: latest_version.to_string(),
                release_notes: latest.body.clone().unwrap_or_default(),
            });
        }
    }

    Ok(UpdateStatus::NoUpdateAvailable)
}

/// Install the latest update
fn install_update() -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");

    let status = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name("quickctx")
        .current_version(current_version)
        .build()
        .map_err(|e| QuickctxError::SelfUpdate(format!("failed to configure update: {}", e)))?
        .update()
        .map_err(|e| QuickctxError::SelfUpdate(format!("failed to install update: {}", e)))?;

    debug!("Update status: {:?}", status);
    Ok(())
}

/// Check for updates in the background and notify if available (non-blocking)
pub fn check_for_update_background() -> Result<()> {
    let last_check_path = get_last_check_file_path()?;

    // Check if we should run the update check
    if let Ok(metadata) = fs::metadata(&last_check_path)
        && let Ok(modified) = metadata.modified()
        && let Ok(elapsed) = SystemTime::now().duration_since(modified)
        && elapsed < Duration::from_secs(UPDATE_CHECK_INTERVAL_DAYS * 24 * 60 * 60)
    {
        // Too soon to check again
        debug!(
            "Skipping update check (last checked {} days ago)",
            elapsed.as_secs() / 86400
        );
        return Ok(());
    }

    // Perform the check
    debug!("Running background update check");
    match check_for_update() {
        Ok(UpdateStatus::UpdateAvailable { version, .. }) => {
            println!(
                "ℹ Update available: {} (run 'quickctx update' to install)",
                version
            );
        }
        Ok(UpdateStatus::NoUpdateAvailable) => {
            debug!("No update available");
        }
        Err(e) => {
            // Don't fail the entire operation if update check fails
            warn!("Background update check failed: {}", e);
        }
    }

    // Update the last check timestamp
    if let Some(parent) = last_check_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            QuickctxError::SelfUpdate(format!("failed to create config dir: {}", e))
        })?;
    }
    fs::write(&last_check_path, b"").map_err(|e| {
        QuickctxError::SelfUpdate(format!("failed to update check timestamp: {}", e))
    })?;

    Ok(())
}

/// Get the path to the file that stores the last update check timestamp
fn get_last_check_file_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir().ok_or_else(|| {
        QuickctxError::SelfUpdate("failed to determine config directory".to_string())
    })?;

    Ok(config_dir.join("quickctx").join("last-update-check"))
}

#[derive(Debug)]
enum UpdateStatus {
    NoUpdateAvailable,
    UpdateAvailable {
        version: String,
        release_notes: String,
    },
}
