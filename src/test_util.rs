#[cfg(test)]
use std::sync::Once;

#[cfg(test)]
static INIT: Once = Once::new();

#[cfg(test)]
pub(crate) fn setup_log() {
    INIT.call_once(|| {
        use tracing::Level;
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let filter = tracing_subscriber::filter::Targets::new()
            .with_default(Level::DEBUG)
            .with_target("nom_exif", Level::ERROR);
        let registry_layer = tracing_subscriber::fmt::layer().with_target(false);
        tracing_subscriber::registry()
            .with(registry_layer)
            .with(filter)
            .init();
    });
}
