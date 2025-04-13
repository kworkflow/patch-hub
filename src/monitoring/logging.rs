use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use multi_log_file_writer::{create_non_blocking_writer, get_fmt_layer, MultiLogFileWriter};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{reload::Handle, Layer, Registry};

pub mod multi_log_file_writer;

const LATEST_LOG_FILENAME: &str = "latest.tracing.log";
const TEMP_LOG_DIR: &str = "/tmp/temporary-patch-hub-logs";

pub struct InitLoggingLayerProduct {
    pub logging_layer: Box<dyn Layer<Registry> + Send + Sync>,
    pub multi_log_file_writer: MultiLogFileWriter,
    pub guards_by_file_name: HashMap<String, WorkerGuard>,
    pub reload_handle: Handle<Box<dyn Layer<Registry> + Send + Sync>, Registry>,
}
struct InitLoggingFileWritersProduct {
    multi_log_file_writer: MultiLogFileWriter,
    guards_by_file_name: HashMap<String, WorkerGuard>,
}

pub fn init_logging_layer() -> InitLoggingLayerProduct {
    let InitLoggingFileWritersProduct {
        multi_log_file_writer,
        guards_by_file_name,
    } = init_logging_file_writers();

    let fmt_layer = get_fmt_layer(multi_log_file_writer.clone());

    let (reload_layer, reload_handle) = tracing_subscriber::reload::Layer::new(fmt_layer);

    InitLoggingLayerProduct {
        logging_layer: Box::new(reload_layer),
        multi_log_file_writer,
        guards_by_file_name,
        reload_handle,
    }
}

fn init_logging_file_writers() -> InitLoggingFileWritersProduct {
    let timestamp_log_file_name = format!(
        "patch-hub_{}.tracing.log",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );

    // logging thread should be non-blocking so it does not interfere with the rest of the application
    let (timestamp_log_writer, timestamp_log_writer_guard) =
        create_non_blocking_writer(TEMP_LOG_DIR, timestamp_log_file_name.as_str());
    let (latest_log_writer, latest_log_writer_guard) =
        create_non_blocking_writer(TEMP_LOG_DIR, LATEST_LOG_FILENAME);

    let writer_by_file_name = HashMap::from([
        (
            timestamp_log_file_name.clone(),
            Arc::new(Mutex::new(timestamp_log_writer)),
        ),
        (
            LATEST_LOG_FILENAME.to_string(),
            Arc::new(Mutex::new(latest_log_writer)),
        ),
    ]);

    let guards_by_file_name = HashMap::from([
        (timestamp_log_file_name, timestamp_log_writer_guard),
        (LATEST_LOG_FILENAME.to_string(), latest_log_writer_guard),
    ]);

    InitLoggingFileWritersProduct {
        multi_log_file_writer: MultiLogFileWriter::new(
            TEMP_LOG_DIR.to_string(),
            writer_by_file_name,
        ),
        guards_by_file_name,
    }
}
