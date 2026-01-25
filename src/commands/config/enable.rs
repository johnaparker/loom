//! Enable subcommand implementation.

use anyhow::Result;

use super::ConfigTarget;
use crate::cli::{ConfigFeature, ConfigScope};

use super::claude_setup;
use super::github_setup;
use super::linear_setup;

/// Run the enable command for a feature.
pub fn run(feature: ConfigFeature, scope: ConfigScope) -> Result<()> {
    let target = ConfigTarget::resolve(scope)?;

    match feature {
        ConfigFeature::Claude => claude_setup::enable(&target),
        ConfigFeature::Github => github_setup::enable(&target),
        ConfigFeature::Linear => linear_setup::enable(&target),
    }
}
