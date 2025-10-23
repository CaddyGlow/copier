use camino::{Utf8Path, Utf8PathBuf};
use ignore::WalkBuilder;

use crate::config::AggregateConfig;

/// Configuration builder for setting up a directory walker with appropriate
/// gitignore handling and custom ignore files.
pub struct WalkerConfigBuilder {
    root: Utf8PathBuf,
    respect_gitignore: bool,
    ignore_files: Vec<Utf8PathBuf>,
}

impl WalkerConfigBuilder {
    /// Creates a new walker configuration builder for the given root directory.
    pub fn new(root: &Utf8Path) -> Self {
        Self {
            root: root.to_owned(),
            respect_gitignore: true,
            ignore_files: Vec::new(),
        }
    }

    /// Creates a walker configuration from an AggregateConfig.
    pub fn from_config(root: &Utf8Path, config: &AggregateConfig) -> Self {
        Self {
            root: root.to_owned(),
            respect_gitignore: config.respect_gitignore,
            ignore_files: config.ignore_files.clone(),
        }
    }

    /// Sets whether to respect gitignore files.
    pub fn respect_gitignore(mut self, value: bool) -> Self {
        self.respect_gitignore = value;
        self
    }

    /// Adds a custom ignore file to the configuration.
    pub fn add_ignore_file(mut self, path: Utf8PathBuf) -> Self {
        self.ignore_files.push(path);
        self
    }

    /// Adds multiple custom ignore files to the configuration.
    pub fn add_ignore_files<I>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = Utf8PathBuf>,
    {
        self.ignore_files.extend(paths);
        self
    }

    /// Builds and configures a WalkBuilder with the specified settings.
    pub fn build(self) -> WalkBuilder {
        let mut builder = WalkBuilder::new(self.root);

        // Basic walker configuration
        builder.follow_links(false);
        builder.sort_by_file_name(|a, b| a.cmp(b));
        builder.standard_filters(true);

        // Gitignore configuration
        if self.respect_gitignore {
            builder.git_ignore(true);
            builder.git_global(true);
            builder.git_exclude(true);
            builder.require_git(false);
        } else {
            builder.git_ignore(false);
            builder.git_global(false);
            builder.git_exclude(false);
        }

        // Add custom ignore files
        for ignore_file in &self.ignore_files {
            builder.add_ignore(ignore_file);
        }

        builder
    }
}
