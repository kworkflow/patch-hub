use std::collections::HashMap;

use logging::{
    init_logging_layer, multi_log_file_writer::MultiLogFileWriter, InitLoggingLayerProduct,
};
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    filter::Targets, layer::SubscriberExt, reload::Handle, util::SubscriberInitExt, Layer, Registry,
};

pub mod logging;

pub const PATH_HUB_TARGET: &str = "patch_hub";

pub struct InitMonitoringProduct {
    pub multi_log_file_writer: MultiLogFileWriter,
    pub logging_guards_by_file_name: HashMap<String, WorkerGuard>,
    pub logging_reload_handle: Handle<Box<dyn Layer<Registry> + Send + Sync>, Registry>,
}

pub fn init_monitoring() -> InitMonitoringProduct {
    let InitLoggingLayerProduct {
        logging_layer,
        multi_log_file_writer,
        guards_by_file_name: logging_guards_by_file_name,
        reload_handle: logging_reload_handle,
    } = init_logging_layer();

    // the filter is separate from the logging layer because of a lib limitation: https://github.com/tokio-rs/tracing/issues/1629
    // otherwise we could not reload the logging layer after the logging dir is updated
    let filter_patch_hub_target = Targets::new()
        .with_target(PATH_HUB_TARGET, LevelFilter::TRACE)
        .with_default(LevelFilter::OFF);

    // for future telemetry monitoring, we should just have to add another .with() in the registry
    // with the new telemetry layer
    tracing_subscriber::registry()
        .with(logging_layer)
        .with(filter_patch_hub_target)
        .init();

    InitMonitoringProduct {
        multi_log_file_writer,
        logging_guards_by_file_name,
        logging_reload_handle,
    }
}
