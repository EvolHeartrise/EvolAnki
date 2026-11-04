// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use once_cell::sync::OnceCell;
use tracing::subscriber::set_global_default;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::EnvFilter;

use crate::prelude::*;

/// Enable logging to the console.
pub fn set_global_logger() -> Result<()> {
    if std::env::var("BURN_LOG").is_ok() {
        return Ok(());
    }
    static ONCE: OnceCell<()> = OnceCell::new();
    ONCE.get_or_try_init(|| -> Result<()> {
        let subscriber = tracing_subscriber::registry()
            .with(fmt::layer().with_target(false))
            .with(EnvFilter::from_default_env());
        set_global_default(subscriber).or_invalid("global subscriber already set")?;
        Ok(())
    })?;
    Ok(())
}
