use std::{
    fmt::Display,
    fs::{self, File, OpenOptions},
    io::Write,
};

use chrono::Local;

use crate::app::config::Config;

const LATEST_LOG_FILENAME: &str = "latest.log";

static mut LOG_BUFFER: Logger = Logger {
    log_file: None,
    log_filepath: None,
    latest_log_file: None,
    latest_log_filepath: None,
    logs_to_print: Vec::new(),
    print_level: LogLevel::Warning,
};

/// Describes the log level of a message
///
/// This is used to determine the severity of a log message so the logger handles it accordingly to the verbosity level.
///
/// The levels severity are: `Info` < `Warning` < `Error`
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LogMessage {
    level: LogLevel,
    message: String,
}

/// The Logger singleton that manages logging to [`stderr`] (log buffer) and a log file.
/// This is safe to use only in single-threaded scenarios. The messages are written to the log file immediatly,
/// but the messages to the `stderr` are written only after the TUI is closed, so they are kept in memory.
///
/// The logger also has a log level that can be set to filter the messages that are written to the log file.
/// Only messages with a level equal or higher than the log level are written to the log file.
///
/// The expected flow is:
///  - Initialize the log file with [`init_log_file`]
///  - Write to the log file with [`info`], [`warn`] or [`error`]
///  - Flush the log buffer to the stderr and close the log file with [`flush`]
///
/// The log file is created in the logs_path defined in the [`Config`] struct
///
/// [`Config`]: super::config::Config
/// [`init_log_file`]: Logger::init_log_file
/// [`info`]: Logger::info
/// [`warn`]: Logger::warn
/// [`error`]: Logger::error
/// [`flush`]: Logger::flush
/// [`stderr`]: std::io::stderr
#[derive(Debug)]
pub struct Logger {
    log_file: Option<File>,
    log_filepath: Option<String>,
    latest_log_file: Option<File>,
    latest_log_filepath: Option<String>,
    logs_to_print: Vec<LogMessage>,
    print_level: LogLevel, // TODO: Add a log level configuration
}

impl Logger {
    /// Private method to get access to the Logger singleton
    ///
    /// This function makes use of unsafe code to access a static mut. Also, it's `inline` so won't have any overhead
    ///
    /// # Safety
    ///
    /// It's safe to use in single-threaded scenarios only
    ///
    /// # Examples
    /// ```rust norun
    /// // Get the logger singleton
    /// Logger::init_log_file(&config); // Initialize the log file
    /// Logger::info("This is an info log message"); // Write a message to the log file
    /// ```
    #[inline]
    fn get_logger() -> &'static mut Logger {
        #[allow(static_mut_refs)]
        unsafe {
            &mut LOG_BUFFER
        }
    }

    /// Write the string `msg` to the logs to print buffer and the log file
    ///
    /// # Panics
    ///
    /// If the log file is not initialized
    ///
    /// # Examples
    /// ```rust norun
    /// // Make sure to initialize the log file before writing to it
    /// Logger::init_log_file(&config);
    /// // Get the logger singleton and write a message to the log file
    /// Logger::get_logger().log(LogLevel::Info, "This is a log message");
    /// ```
    fn log<M: Display>(&mut self, level: LogLevel, message: M) {
        let current_datetime = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let message = format!("[{}] {}", current_datetime, message);

        let log = LogMessage { level, message };

        let file = self.log_file
            .as_mut()
            .expect("Log file not initialized, make sure to call Logger::init_log_file() before writing to the log file");
        writeln!(file, "{log}").expect("Failed to write to log file");

        let latest_log_file = self.latest_log_file
        .as_mut()
        .expect("Latest log file not initialized, make sure to call Logger::init_log_file() before writing to the log file");
        writeln!(latest_log_file, "{log}").expect("Failed to write to real time log file");

        if self.print_level <= level {
            // Only save logs to print w/ level equal or higher than the filter log level
            self.logs_to_print.push(log);
        }
    }

    /// Write an info message to the log
    ///
    /// # Panics
    ///
    /// If the log file is not initialized
    ///
    /// # Safety
    ///
    /// It's safe to use in single-threaded scenarios only
    ///
    /// # Examples
    ///
    /// ```rust norun
    ///
    /// // Make sure to initialize the log file before writing to it
    /// Logger::init_log_file(&config);
    /// Logger::info("This is an info message"); // [INFO] [2024-09-11 14:59:00] This is an info message
    /// ```
    #[inline]
    #[allow(dead_code)]
    pub fn info<M: Display>(msg: M) {
        Logger::get_logger().log(LogLevel::Info, msg);
    }

    /// Write a warn message to the log
    ///
    /// # Panics
    ///
    /// If the log file is not initialized
    ///
    /// # Safety
    ///
    /// It's safe to use in single-threaded scenarios only
    ///
    /// # Examples
    ///
    /// ```rust norun
    ///
    /// // Make sure to initialize the log file before writing to it
    /// Logger::init_log_file(&config);
    /// Logger::warn("This is a warning"); // [WARN] [2024-09-11 14:59:00] This is a warning
    /// ```
    #[inline]
    #[allow(dead_code)]
    pub fn warn<M: Display>(msg: M) {
        Logger::get_logger().log(LogLevel::Warning, msg);
    }

    /// Write an error message to the log
    ///
    /// # Panics
    ///
    /// If the log file is not initialized
    ///
    /// # Safety
    ///
    /// It's safe to use in single-threaded scenarios only
    ///
    /// # Examples
    ///
    /// ```rust norun
    ///
    /// // Make sure to initialize the log file before writing to it
    /// Logger::init_log_file(&config);
    /// Logger::error("This is an error message"); // [ERROR] [2024-09-11 14:59:00] This is an error message
    /// ```
    #[inline]
    #[allow(dead_code)]
    pub fn error<M: Display>(msg: M) {
        Logger::get_logger().log(LogLevel::Error, msg);
    }

    /// Flush the log buffer to stderr and closes the log file.
    /// It's intended to be called only once when patch-hub is finishing.
    ///
    /// # Panics
    ///
    /// If called before the log file is initialized or if called twice
    ///
    /// # Examples
    /// ```rust norun
    /// // Make sure to initialize the log file before writing to it
    /// Logger::init_log_file(&config);
    ///
    /// // Flush before finishing the application
    /// Logger::flush();
    /// // Any further attempt to use the logger will panic, unless it's reinitialized
    /// ```
    pub fn flush() {
        let logger = Logger::get_logger();
        for entry in &logger.logs_to_print {
            eprintln!("{}", entry);
        }

        if let Some(f) = &logger.log_filepath {
            eprintln!("Check the full log file: {}", f);
        }
    }

    /// Initialize the log file.
    ///
    /// This function must be called before any other operation with the logging system
    ///
    /// # Panics
    ///
    /// If it fails to create the log file
    ///
    /// # Examples
    /// ```rust norun
    /// // Once you get the config struct...
    /// let config = Config::build();
    /// // ... initialize the log file
    /// Logger::init_log_file(&config);
    /// ```
    pub fn init_log_file(config: &Config) -> Result<(), std::io::Error> {
        let logger = Logger::get_logger();

        let logs_path = config.logs_path();
        fs::create_dir_all(logs_path)?;

        if logger.latest_log_file.is_none() {
            let latest_log_filename = LATEST_LOG_FILENAME.to_string();
            let latest_log_filepath = format!("{}/{}", logs_path, latest_log_filename);

            File::create(&latest_log_filepath)?;

            let log_file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&latest_log_filepath)?;

            logger.latest_log_file = Some(log_file);
            logger.latest_log_filepath = Some(latest_log_filepath);
        }

        if logger.log_file.is_none() {
            let log_filename = format!(
                "patch-hub_{}.log",
                chrono::Local::now().format("%Y%m%d-%H%M%S")
            );
            let log_filepath = format!("{}/{}", logs_path, log_filename);

            let log_file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_filepath)?;

            logger.log_file = Some(log_file);
            logger.log_filepath = Some(log_filepath);
        }

        Ok(())
    }
}

impl Display for LogMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.level, self.message)
    }
}

impl Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warning => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}
