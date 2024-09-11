use std::{
    fmt::Display, fs::{self, File, OpenOptions}, io::Write
};

use super::config::Config;
use chrono::Local;

static mut LOG_BUFFER: Logger = Logger {
    logs: Vec::new(),
    log_file: None,
    log_filepath: None,
    level: LogLevel::Info,
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
    logs: Vec<LogMessage>,
    log_file: Option<File>,
    log_filepath: Option<String>,
    level: LogLevel, // TODO: Add a log level configuration
}

impl Logger {
    /// Get access to the Logger singleton
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
    /// Logger::logger().init_log_file(&config); // Initialize the log file
    /// Logger::logger().write("This is a log message"); // Write a message to the log file
    /// ```
    #[inline]
    pub fn logger() -> &'static mut Logger {
        #[allow(static_mut_refs)]
        unsafe {
            &mut LOG_BUFFER
        }
    }

    /// Write the string `msg` to the log buffer and the log file
    ///
    /// # Panics
    ///
    /// If the log file is not initialized
    ///
    /// # Examples
    /// ```rust norun
    /// // Make sure to initialize the log file before writing to it
    /// Logger::logger().init_log_file(&config);
    /// // Get the logger singleton and write a message to the log file
    /// Logger::logger().write("This is a log message");
    /// ```
    fn log(&mut self, level: LogLevel, message: &str) {
        let current_datetime = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let message = format!("[{}] {}", current_datetime, message);
        
        let log = LogMessage { level, message };

        let file = self.log_file.as_mut().expect("Log file not initialized, make sure to call Logger::init_log_file() before writing to the log file");
        
        // TODO: Colorize the log messages
        writeln!(file, "{}", &log.message).expect("Failed to write to log file");

        if self.level <= level { // Only save logs with level equal or higher than the filter log level
            self.logs.push(log);
        }
    }

    /// Write an info message to the log 
    /// 
    /// # Panics
    /// 
    /// If the log file is not initialized
    /// 
    /// # Examples
    /// 
    /// ```rust norun
    /// 
    /// // Make sure to initialize the log file before writing to it
    /// Logger::logger().init_log_file(&config);
    /// Logger::info("This is an info message"); // [INFO] [2024-09-11 14:59:00] This is an info message
    /// ```
    #[inline]
    #[allow(dead_code)]
    pub fn info(msg: &str) {
        Logger::logger().log(LogLevel::Info, msg);
    }

    /// Write a warn message to the log 
    /// 
    /// # Panics
    /// 
    /// If the log file is not initialized
    /// 
    /// # Examples
    /// 
    /// ```rust norun
    /// 
    /// // Make sure to initialize the log file before writing to it
    /// Logger::logger().init_log_file(&config);
    /// Logger::warn("This is a warning"); // [WARN] [2024-09-11 14:59:00] This is a warning
    /// ```
    #[inline]
    #[allow(dead_code)]
    pub fn warn(msg: &str) {
        Logger::logger().log(LogLevel::Warning, msg);
    }

    /// Write an error message to the log 
    /// 
    /// # Panics
    /// 
    /// If the log file is not initialized
    /// 
    /// # Examples
    /// 
    /// ```rust norun
    /// 
    /// // Make sure to initialize the log file before writing to it
    /// Logger::logger().init_log_file(&config);
    /// Logger::info("This is an error message"); // [ERROR] [2024-09-11 14:59:00] This is an error message
    /// ```
    #[inline]
    #[allow(dead_code)]
    pub fn error(msg: &str) {
        Logger::logger().log(LogLevel::Error, msg);
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
    /// Logger::logger().init_log_file(&config);
    ///
    /// // Flush before finishing the application
    /// Logger::logger().flush();
    /// // Any further attempt to use the logger will panic, unless it's reinitialized
    /// ```
    pub fn flush(&mut self) {
        for entry in &self.logs {
            eprintln!("{}", entry);
        }

        if let Some(f) = &self.log_filepath {
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
    /// Logger::logger().init_log_file(&config);
    /// ```
    pub fn init_log_file(&mut self, config: &Config) {
        if let None = self.log_file {
            let logs_path = &config.logs_path;
            fs::create_dir_all(logs_path)
                .expect(&format!("Failed to create the logs folder at {}", logs_path));

            let log_filename = format!(
                "patch-hub_{}.log",
                chrono::Local::now().format("%Y%m%d-%H%M%S")
            );
            
            let fullpath = format!("{}/{}", logs_path, log_filename);
            self.log_file = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&fullpath)
                    .expect(&format!("Failed to create the log file at {}", fullpath)),
            );
            self.log_filepath = Some(fullpath);
        }
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