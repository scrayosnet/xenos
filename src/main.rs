use std::borrow::Cow::Owned;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;
use xenos::config::Config;

/// Starts the Xenos application. It reads the application [Config], initializes [sentry] and [tracing]
/// and starts the Xenos service.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read config from config files and environment variables
    let config = Arc::new(Config::new()?);

    // initialize sentry
    let _sentry = sentry::init((
        config
            .sentry
            .enabled
            .then_some(config.sentry.address.clone()),
        sentry::ClientOptions {
            debug: config.sentry.debug,
            release: sentry::release_name!(),
            environment: Some(Owned(config.sentry.environment.clone())),
            ..sentry::ClientOptions::default()
        },
    ));

    // initialize logging with the sentry hook
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .compact()
                .with_filter(EnvFilter::from_default_env()),
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
        .block_on(async { xenos::start(config).await })
}
