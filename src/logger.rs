use std::{
    fmt::{Debug, Display},
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    str::FromStr,
};

use actix::{Actor, Addr, Handler, Message};
use chrono::Local;
use thiserror::Error;

const LATEST_LOG_FILENAME: &str = "latest.log";

/// Describes the log level of a message
///
/// This is used to determine the severity of a log message so the logger handles it accordingly to the verbosity level.
///
/// The levels severity are: `Info` < `Warning` < `Error`
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub enum LogLevel {
    #[default]
    Info,
    Warning,
    Error,
}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
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
    log_file: File,
    log_filepath: PathBuf,
    latest_log_file: File,
    latest_log_filepath: PathBuf,
    logs_to_print: Vec<LogMessage>,
    print_level: LogLevel, // TODO: Add a log level configuration
}

impl Actor for Logger {
    type Context = actix::Context<Self>;

    fn started(&mut self, _: &mut Self::Context) {
        self.log(LogLevel::Info, "patch-hub started");
    }
}

impl Logger {
    pub fn new(path: &str, level: LogLevel) -> Result<Logger, LoggerInitError> {
        fs::create_dir_all(path)?;

        let timestamp = Local::now().format("%Y%m%d-%H%M%S");

        let log_filepath = PathBuf::from_str(path)?.join(format!("patch-hub_{}.log", timestamp));
        let latest_log_filepath = PathBuf::from_str(path)?.join(LATEST_LOG_FILENAME);

        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_filepath)?;
        let latest_log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&latest_log_filepath)?;

        Ok(Logger {
            log_file,
            log_filepath,
            latest_log_file,
            latest_log_filepath,
            logs_to_print: Vec::new(),
            print_level: level,
        })
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

        writeln!(self.log_file, "{log}").expect("Failed to write to log file");

        writeln!(self.latest_log_file, "{log}").expect("Failed to write to real time log file");

        if self.print_level <= level {
            // Only save logs to print w/ level equal or higher than the filter log level
            self.logs_to_print.push(log);
        }
    }

    /// Logs a result if it's an error. Will always return the result
    fn log_on_error<T: 'static, E: Display + 'static>(
        &mut self,
        level: LogLevel,
        result: Result<T, E>,
    ) -> Result<T, E> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                self.log(level, &error);
                Err(error)
            }
        }
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
    pub fn flush(&mut self) {
        for entry in &self.logs_to_print {
            eprintln!("{}", entry);
        }

        eprintln!(
            "Check the full log file: {} or {}",
            self.log_filepath.display(),
            self.latest_log_filepath.display()
        )
    }

    pub fn collect_garbage(&mut self, max_age: usize) {
        if max_age == 0 {
            return;
        }

        let now = std::time::SystemTime::now();
        let Some(path) = self.log_filepath.parent() else {
            self.log(
                LogLevel::Error,
                "Failed to read the logs directory during garbage collection",
            );
            return;
        };
        let Ok(logs) = std::fs::read_dir(path) else {
            self.log(
                LogLevel::Error,
                "Failed to read the logs directory during garbage collection",
            );
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

            if age as usize > max_age && std::fs::remove_file(log.path()).is_err() {
                self.log(
                    LogLevel::Warning,
                    format!(
                        "Failed to remove the log file: {}",
                        log.path().to_string_lossy()
                    ),
                );
            }
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

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Message)]
#[rtype(result = "()")]
pub struct Dbg<D: Debug>(pub D);

/// Logs an info message. The message is any value that implements [`Display`]
///
/// The message is always written to the log file, but only will be shown on exit
/// if the log level is [`LogLevel::Info`].
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Message)]
#[rtype(result = "()")]
pub struct Info<M: Display>(pub M);

/// Logs a warning message. The message is any value that implements [`Display`]
///
/// The message is always written to the log file, but only will be shown on exit
/// if the log level is [`LogLevel::Warning`] or lower.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Message)]
#[rtype(result = "()")]
pub struct Warn<M: Display>(pub M);

/// Logs an error message. The message is any value that implements [`Display`]
///
/// The message is always written to the log file and shown on exit
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Message)]
#[rtype(result = "()")]
pub struct Error<M: Display>(pub M);

/// Logs an info message if the value is an [`Err`].
///
/// The message is always written to the log file, but only will be shown on exit
/// if the log level is [`LogLevel::Info`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Message)]
#[rtype(result = "Result<T, E>")]
pub struct InfoOnError<T: 'static, E: Display + 'static>(pub Result<T, E>);

/// Logs a warning message if the value is an [`Err`].
///
/// The message is always written to the log file, but only will be shown on exit
/// if the log level is [`LogLevel::Warning`] or lower.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Message)]
#[rtype(result = "Result<T, E>")]
pub struct WarnOnError<T: 'static, E: Display + 'static>(pub Result<T, E>);

/// Logs an error message if the value is an [`Err`].
///
/// The message is always written to the log file and shown on exit
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Message)]
#[rtype(result = "Result<T, E>")]
pub struct ErrorOnError<T: 'static, E: Display + 'static>(pub Result<T, E>);

impl<M: Display> Handler<Info<M>> for Logger {
    type Result = ();
    fn handle(&mut self, Info(msg): Info<M>, _: &mut Self::Context) -> Self::Result {
        self.log(LogLevel::Info, msg);
    }
}

impl<M: Display> Handler<Warn<M>> for Logger {
    type Result = ();
    fn handle(&mut self, Warn(msg): Warn<M>, _: &mut Self::Context) -> Self::Result {
        self.log(LogLevel::Warning, msg);
    }
}

impl<M: Display> Handler<Error<M>> for Logger {
    type Result = ();
    fn handle(&mut self, Error(msg): Error<M>, _: &mut Self::Context) -> Self::Result {
        self.log(LogLevel::Error, msg);
    }
}

impl<T: 'static, E: Display + 'static> Handler<InfoOnError<T, E>> for Logger {
    type Result = Result<T, E>;
    fn handle(
        &mut self,
        InfoOnError(result): InfoOnError<T, E>,
        _: &mut Self::Context,
    ) -> Self::Result {
        self.log_on_error(LogLevel::Info, result)
    }
}

impl<T: 'static, E: Display + 'static> Handler<WarnOnError<T, E>> for Logger {
    type Result = Result<T, E>;
    fn handle(
        &mut self,
        WarnOnError(result): WarnOnError<T, E>,
        _: &mut Self::Context,
    ) -> Self::Result {
        self.log_on_error(LogLevel::Warning, result)
    }
}

impl<T: 'static, E: Display + 'static> Handler<ErrorOnError<T, E>> for Logger {
    type Result = Result<T, E>;
    fn handle(
        &mut self,
        ErrorOnError(result): ErrorOnError<T, E>,
        _: &mut Self::Context,
    ) -> Self::Result {
        self.log_on_error(LogLevel::Error, result)
    }
}

/// Finishes the logger by flushing the logs to the terminal and closing the files
///
/// TODO: Receive the address of the terminal to ask it to Detach before flushing
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Message)]
#[rtype(result = "()")]
pub struct Flush;

impl Handler<Flush> for Logger {
    type Result = ();
    fn handle(&mut self, _: Flush, _: &mut Self::Context) -> Self::Result {
        self.flush()
    }
}

/// Deletes old log files, the only argument is the maximum age of the log files in days
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Message)]
#[rtype(result = "()")]
pub struct CollectGarbage(pub usize);

impl Handler<CollectGarbage> for Logger {
    type Result = ();
    fn handle(
        &mut self,
        CollectGarbage(max_age): CollectGarbage,
        _: &mut Self::Context,
    ) -> Self::Result {
        self.collect_garbage(max_age)
    }
}

/// Possible errors returned from the [`Logger::new`] method
#[derive(Debug, Error)]
pub enum LoggerInitError {
    PathErr(<PathBuf as FromStr>::Err),
    IOErr(std::io::Error),
}

impl From<std::io::Error> for LoggerInitError {
    fn from(value: std::io::Error) -> Self {
        Self::IOErr(value)
    }
}

impl From<std::convert::Infallible> for LoggerInitError {
    fn from(value: std::convert::Infallible) -> Self {
        Self::PathErr(value)
    }
}

impl Display for LoggerInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoggerInitError::PathErr(error) => write!(f, "{error}"),
            LoggerInitError::IOErr(error) => write!(f, "{error}"),
        }
    }
}

#[allow(dead_code)]
pub trait LoggerActor {
    async fn info<M: Display + Send + 'static>(&self, message: M);
    async fn warn<M: Display + Send + 'static>(&self, message: M);
    async fn error<M: Display + Send + 'static>(&self, message: M);
    async fn info_on_error<T: Send + 'static, E: Display + Send + 'static>(
        &self,
        result: Result<T, E>,
    ) -> Result<T, E>;
    async fn warn_on_error<T: Send + 'static, E: Display + Send + 'static>(
        &self,
        result: Result<T, E>,
    ) -> Result<T, E>;
    async fn error_on_error<T: Send + 'static, E: Display + Send + 'static>(
        &self,
        result: Result<T, E>,
    ) -> Result<T, E>;
    async fn flush(&self);
    async fn collect_garbage(&self, max_age: usize);
}

impl LoggerActor for Addr<Logger> {
    async fn info<M: Display + Send + 'static>(&self, message: M) {
        self.send(Info(message))
            .await
            .expect("Failed to log an info. Logger actor is dead")
    }
    async fn warn<M: Display + Send + 'static>(&self, message: M) {
        self.send(Warn(message))
            .await
            .expect("Failed to log a warning. Logger actor is dead")
    }

    async fn error<M: Display + Send + 'static>(&self, message: M) {
        self.send(Error(message))
            .await
            .expect("Failed to log an error. Logger actor is dead")
    }

    async fn info_on_error<T: Send + 'static, E: Display + Send + 'static>(
        &self,
        result: Result<T, E>,
    ) -> Result<T, E> {
        self.send(InfoOnError(result))
            .await
            .expect("Failed to log an info on error. Logger actor is dead")
    }

    async fn warn_on_error<T: Send + 'static, E: Display + Send + 'static>(
        &self,
        result: Result<T, E>,
    ) -> Result<T, E> {
        self.send(WarnOnError(result))
            .await
            .expect("Failed to log a warning on error. Logger actor is dead")
    }

    async fn error_on_error<T: Send + 'static, E: Display + Send + 'static>(
        &self,
        result: Result<T, E>,
    ) -> Result<T, E> {
        self.send(ErrorOnError(result))
            .await
            .expect("Failed to log an error on error. Logger actor is dead")
    }

    async fn flush(&self) {
        self.send(Flush)
            .await
            .expect("Failed to flush the logger. Logger actor is dead")
    }

    async fn collect_garbage(&self, max_age: usize) {
        self.send(CollectGarbage(max_age))
            .await
            .expect("Failed to collect garbage. Logger actor is dead")
    }
}
