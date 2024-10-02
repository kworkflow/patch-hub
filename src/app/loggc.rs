//! Log Garbage Collector
//!
//! This module is responsible for cleaning up the log files.
use super::{config::Config, logging::Logger};

/// Collects the garbage from the logs directory.
/// Will check for log files `patch-hub_*.log` and remove them if they are older than the `max_log_age` in the config.
pub fn collect_garbage(config: &Config) {
    if config.max_log_age() == &0 {
        return;
    }

    let now = std::time::SystemTime::now();
    let logs_path = config.logs_path();
    let Ok(logs) = std::fs::read_dir(logs_path) else {
        Logger::error("Failed to read the logs directory during garbage collection");
        return;
    };

    for log in logs {
        let Ok(log) = log else {
            continue;
        };
        let filename = log.file_name();

        if !filename.to_string_lossy().ends_with(".log")
            || !filename.to_string_lossy().starts_with("patch-hub_")
        {
            continue;
        }

        let Ok(Ok(created_date)) = log.metadata().map(|meta| meta.created()) else {
            continue;
        };
        let Ok(age) = now.duration_since(created_date) else {
            continue;
        };
        let age = age.as_secs() / 60 / 60 / 24;

        if age > *config.max_log_age() && std::fs::remove_file(log.path()).is_err() {
            Logger::warn(&format!(
                "Failed to remove the log file: {}",
                log.path().to_string_lossy()
            ));
        }
    }
}
