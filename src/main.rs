use std::borrow::Cow::Owned;
use std::sync::Arc;
use tracing::{info, Level};

use tracing_subscriber::prelude::*;
use xenos::settings::Settings;

/// Starts the Xenos application. It reads the application [Settings], initializes [sentry] and [tracing]
/// and starts the Xenos service.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read settings from config files and environment variables
    let settings = Arc::new(Settings::new()?);

    // initialize sentry
    let _sentry = sentry::init((
        settings
            .sentry
            .enabled
            .then_some(settings.sentry.address.clone()),
        sentry::ClientOptions {
            debug: settings.debug,
            release: sentry::release_name!(),
            environment: Some(Owned(settings.sentry.environment.clone())),
            ..sentry::ClientOptions::default()
        },
    ));

    // initialize logging with sentry hook
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::Layer::new()
                .with_writer(std::io::stdout.with_max_level(Level::INFO)),
        )
        .with(sentry_tracing::layer())
        .init();
    if _sentry.is_enabled() {
        info!("sentry is enabled");
    }

    // run xenos blocking
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async { xenos::start(settings).await })
}
