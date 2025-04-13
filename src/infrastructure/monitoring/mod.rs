use logging::{init_logging_layer, InitLoggingLayerProduct};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod logging;

pub struct InitMonitoringProduct {
    pub file_writer_guards: Vec<WorkerGuard>,
}

pub fn init_monitoring() -> InitMonitoringProduct {
    let InitLoggingLayerProduct {
        logging_layer,
        file_writer_guards,
    } = init_logging_layer();

    // for future telemetry monitoring, we should just have to add another .with() in the registry
    // with the new telemetry layer
    tracing_subscriber::registry().with(logging_layer).init();

    InitMonitoringProduct { file_writer_guards }
}
