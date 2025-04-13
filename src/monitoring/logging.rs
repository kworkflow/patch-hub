use std::{
    fs::File,
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{Layer, Registry};

const PATH_HUB_TARGET: &str = "patch_hub";
const LATEST_LOG_FILENAME: &str = "latest.log";
const TEMP_LOG_DIR: &str = "/tmp/temporary-patch-hub-logs";

pub struct InitLoggingLayerProduct {
    pub logging_layer: Box<dyn Layer<Registry> + Send + Sync>,
    pub file_writer_guards: Vec<WorkerGuard>,
}
struct InitLoggingFileWritersProduct {
    multi_file_writer: MultiFileWriter,
    file_writer_guards: Vec<WorkerGuard>,
}

#[derive(Clone)]
pub struct MultiFileWriter {
    writers: Vec<Arc<Mutex<dyn Write + Send>>>,
}

pub fn init_logging_layer() -> InitLoggingLayerProduct {
    let InitLoggingFileWritersProduct {
        multi_file_writer,
        file_writer_guards,
    } = init_logging_file_writers();

    let filter_patch_hub_target =
        tracing_subscriber::filter::filter_fn(|metadata| metadata.target() == PATH_HUB_TARGET);

    let logging_layer = Box::new(
        tracing_subscriber::fmt::layer()
            .with_writer(move || multi_file_writer.clone())
            .with_file(true)
            .with_line_number(true)
            .with_timer(tracing_subscriber::fmt::time::SystemTime)
            .json()
            .with_filter(filter_patch_hub_target),
    );

    InitLoggingLayerProduct {
        logging_layer,
        file_writer_guards,
    }
}

fn init_logging_file_writers() -> InitLoggingFileWritersProduct {
    let timestamp_log_file_name = format!(
        "patch-hub_{}.log",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );

    // we have to truncate the file so if the file already exists we overwrite its content
    let latest_log_path = Path::new(TEMP_LOG_DIR).join(LATEST_LOG_FILENAME);
    let _ = File::create(&latest_log_path);

    let timestamp_log_file_appender =
        tracing_appender::rolling::never(TEMP_LOG_DIR, timestamp_log_file_name);
    let latest_log_file_appender =
        tracing_appender::rolling::never(TEMP_LOG_DIR, LATEST_LOG_FILENAME);

    // logging thread should be non-blocking so it does not interfere with the rest of the application
    let (timestamp_log_writer, timestamp_log_writer_guard) =
        tracing_appender::non_blocking(timestamp_log_file_appender);
    let (latest_log_writer, latest_log_writer_guard) =
        tracing_appender::non_blocking(latest_log_file_appender);

    InitLoggingFileWritersProduct {
        multi_file_writer: MultiFileWriter::new(vec![
            Arc::new(Mutex::new(timestamp_log_writer)),
            Arc::new(Mutex::new(latest_log_writer)),
        ]),
        file_writer_guards: vec![timestamp_log_writer_guard, latest_log_writer_guard],
    }
}

impl MultiFileWriter {
    pub fn new(writers: Vec<Arc<Mutex<dyn Write + Send>>>) -> Self {
        Self { writers }
    }
}

impl Write for MultiFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for writer in &self.writers {
            let _ = writer.lock().expect("to get lock").write_all(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        for writer in &self.writers {
            writer.lock().expect("to get lock").flush()?;
        }
        Ok(())
    }
}
