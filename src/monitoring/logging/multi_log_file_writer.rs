use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
};

use tracing::{event, Level};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::{reload::Handle, Layer, Registry};

use crate::app::config::Config;

#[derive(Clone)]
pub struct MultiLogFileWriter {
    directory: String,
    writer_by_file_name: HashMap<String, Arc<Mutex<NonBlocking>>>,
}

impl MultiLogFileWriter {
    pub fn new(
        directory: String,
        writer_by_file_name: HashMap<String, Arc<Mutex<NonBlocking>>>,
    ) -> Self {
        Self {
            writer_by_file_name,
            directory,
        }
    }

    pub fn update_log_writer_with_config(
        &mut self,
        _config: &Config,
        current_guards_by_file_name: HashMap<String, WorkerGuard>,
        reload_handle: Handle<Box<dyn Layer<Registry> + Send + Sync>, Registry>,
    ) -> Vec<WorkerGuard> {
        let new_log_directory = "./temporary-logs-test/config-dir-logs-test/";
        let guards = self.update_logging_dir(
            new_log_directory,
            current_guards_by_file_name,
            reload_handle,
        );

        event!(
            Level::INFO,
            "Updated log file directory to: {}",
            new_log_directory
        );
        guards
    }

    pub fn update_logging_dir(
        &mut self,
        new_directory: &str,
        mut current_guards_by_file_name: HashMap<String, WorkerGuard>,
        reload_handle: Handle<Box<dyn Layer<Registry> + Send + Sync>, Registry>,
    ) -> Vec<WorkerGuard> {
        event!(Level::INFO, "Starting log directory update...");
        let mut new_guards = vec![];
        let old_directory = self.directory.clone();

        for (file_name, current_writer) in self.writer_by_file_name.iter() {
            let old_log_file_path = format!("{}/{}", old_directory, file_name);
            let new_log_file_path = format!("{}/{}", new_directory, file_name);

            // first we drop current corresponding guard
            if let Some(current_guard) = current_guards_by_file_name.remove(file_name) {
                drop(current_guard);
                // making sure we'll flush everything by the time we copy old file contents
                std::thread::sleep(std::time::Duration::from_millis(50));
            }

            let (file_writer, file_writer_guard) =
                create_non_blocking_writer(new_directory, file_name.as_str());

            Self::copy_old_logs_to_new_path(old_log_file_path, new_log_file_path);

            // then we update the writers
            let mut mutex_guard = current_writer.lock().expect("to get lock");
            *mutex_guard = file_writer;

            new_guards.push(file_writer_guard);
        }

        // reloading layer
        reload_handle
            .reload(get_fmt_layer(self.clone()))
            .expect("Failed reloading logging layer");

        self.directory = new_directory.to_string();

        new_guards
    }

    fn copy_old_logs_to_new_path(old_log_file_path: String, new_log_file_path: String) {
        let Ok(mut old_log_file_content) = File::open(&old_log_file_path) else {
            event!(
                Level::ERROR,
                "Could not open old log file: {}",
                old_log_file_path
            );
            return;
        };
        if let Some(parent_dir) = Path::new(&new_log_file_path).parent() {
            // Create new log dir if it doesn't exist
            std::fs::create_dir_all(parent_dir).expect("to create dir");
        }
        let Ok(mut new_log_file_content) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&new_log_file_path)
        else {
            event!(
                Level::ERROR,
                "Could not open new log file: {}",
                new_log_file_path
            );
            return;
        };

        let copy_result = std::io::copy(&mut old_log_file_content, &mut new_log_file_content);
        if let Err(err) = copy_result {
            event!(Level::ERROR, "Could not copy old file logs: {}", err);
        };
    }
}

impl Write for MultiLogFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for writer in self.writer_by_file_name.values() {
            writer
                .lock()
                .expect("to get lock")
                .write_all(buf)
                .expect("to write");
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        for writer in self.writer_by_file_name.values() {
            writer
                .lock()
                .expect("to get lock")
                .flush()
                .expect("to flush");
        }
        Ok(())
    }
}

pub fn get_fmt_layer(writer: MultiLogFileWriter) -> Box<dyn Layer<Registry> + Send + Sync> {
    tracing_subscriber::fmt::layer()
        .with_writer(move || writer.clone())
        .with_file(true)
        .with_line_number(true)
        .with_timer(tracing_subscriber::fmt::time::SystemTime)
        .json()
        .boxed()
}

pub fn create_non_blocking_writer(directory: &str, file_name: &str) -> (NonBlocking, WorkerGuard) {
    // we have to truncate the file so if the file already exists we overwrite its content
    // this is particularly important for the latest log desired behavior
    let log_path = Path::new(directory).join(file_name);
    let _ = File::create(&log_path);

    let file_appender = tracing_appender::rolling::never(directory, file_name);
    tracing_appender::non_blocking(file_appender)
}
