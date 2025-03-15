use std::{fmt::Display, path::PathBuf};

use color_eyre::eyre::Context;
use tokio::{
    fs::{self, File, OpenOptions},
    io::AsyncWriteExt,
    sync::mpsc::Sender,
    task::JoinHandle,
};

/// Describes the log level of a message
///
/// This is used to determine the severity of a log message so the logger
/// handles it accordingly to the verbosity level.
///
/// The levels severity are: `Info` < `Warning` < `Error`
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// The lowest level, dedicated to regular information that is not really
    /// important
    Info,
    /// Mid level, used to indicate when something went wrong but it's not
    /// critical
    Warning,
    /// The highest level, used to indicate critical errors. But not enought to
    /// crash the program
    Error,
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

/// Describes a message to be logged
///
/// Contains the message constent itself as a [`String`] and its [`LogLevel`]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LogMessage {
    level: LogLevel,
    message: String,
}

impl Display for LogMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.level, self.message)
    }
}

/// The Logger manages logging to [`stderr`] (log buffer) and a log file.
/// The messages are written to the log file immediatly,
/// but the messages to the `stderr` are written only after the TUI is closed,
/// so they are kept in memory.
///
/// The logger also has a log level that can be set to filter the messages that
/// are written to the log file.
/// Only messages with a level equal or higher than the log level are written
/// to the log file.
///
/// You're not supossed to use an instance of this directly, but use
/// [`Logger`] instead by calling [`spawn`] as soon as this struct is built.
///
/// The expected flow is:
///  - Instantiate the logger with [`build`]
///  - Spawn the actor with [`spawn`]
///  - Log with [`info`], [`warn`] or [`error`]
///  - Flush the log buffer to the stderr and finish the logger with [`flush`]
///
/// [`Config`]: super::config::Config
/// [`info`]: Logger::info
/// [`warn`]: Logger::warn
/// [`error`]: Logger::error
/// [`flush`]: Logger::flush
/// [`stderr`]: std::io::stderr
/// [`spawn`]: LoggerCore::spawn
/// [`build`]: LoggerCore::build
#[derive(Debug)]
pub struct LoggerCore {
    log_dir: PathBuf,
    log_file_path: PathBuf,
    log_file: File,
    latest_log_file: File,
    logs_to_print: Vec<LogMessage>,
    print_level: LogLevel, // TODO: Add a log level configuration
    max_age: usize,
}

impl LoggerCore {
    /// Creates a new logger instance. The parameters are the [dir] where the
    /// log files will be stored, [level] of log messages, and [max_age] of the
    /// log files in days.
    ///
    /// You're supposed to call [`spawn`] immediately after this method to
    /// transform the logger instance into an actor.
    ///
    /// # Errors
    ///
    /// If either the latest log file or the log file cannot be created, an
    /// error is returned.
    ///
    /// [`level`]: LogLevel
    /// [`flush`]: LoggerTx::flush
    /// [`spawn`]: Logger::spawn
    pub async fn build(dir: &str, level: LogLevel, max_age: usize) -> color_eyre::Result<Self> {
        let log_dir = PathBuf::from(dir);
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let log_file_path = log_dir.join(format!("patch-hub-{}.log", timestamp));

        let log_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&log_file_path)
            .await
            .context("While building the logger")?;

        let latest_log_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(log_dir.join("latest.log"))
            .await
            .context("While building the logger")?;

        Ok(Self {
            log_dir,
            log_file_path,
            log_file,
            latest_log_file,
            logs_to_print: Vec::new(),
            print_level: level,
            max_age,
        })
    }

    /// Transforms the logger core instance into an actor. This method returns a
    /// [`Logger`] and a [`JoinHandle`] that can be used to send commands to the
    /// logger or await for it to finish (when a [`flush`] is performed, for
    /// instance).
    ///
    /// The handling of the commandds received is done sequentially, so a
    /// command is only processed once the previous one is finished.
    ///
    /// [`flush`]: Logger::flush
    pub fn spawn(mut self) -> (Logger, JoinHandle<()>) {
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        let handle = tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                match command {
                    Command::Log(msg) => {
                        self.log(msg).await;
                    }
                    Command::Flush => {
                        self.flush();
                        rx.close();
                        break;
                    }
                    Command::CollectGarbage => {
                        self.collect_garbage().await;
                    }
                }
            }
        });

        (Logger::Default(tx), handle)
    }

    /// Given a [`LogMessage`] object, writes it to the current and latest log
    /// files. If the message level is high enough, it is also stored in the log
    /// buffer to be printed to [`stderr`] when a [`flush`] is performed.
    ///
    /// [`stderr`]: std::io::stderr
    /// [`flush`]: Logger::flush
    async fn log(&mut self, message: LogMessage) {
        self.log_file
            .write_all(format!("{}\n", &message).as_bytes())
            .await
            .expect("Failed to write to the current log file");

        self.log_file
            .flush()
            .await
            .expect("Failed to flush the current log file");

        self.latest_log_file
            .write_all(format!("{}\n", &message).as_bytes())
            .await
            .expect("Failed to write to the latest log file");

        self.latest_log_file
            .flush()
            .await
            .expect("Failed to flush the latest log file");

        if message.level >= self.print_level {
            self.logs_to_print.push(message);
        }
    }

    /// Writes the log messages to the [`stderr`] if their level is equal or
    /// higher than the print level set in the logger.
    ///
    /// **The logger is destroyed after this method is called.**
    ///
    /// [`stderr`]: std::io::stderr
    fn flush(self) {
        for message in &self.logs_to_print {
            eprintln!("{}", message);
        }

        if !self.logs_to_print.is_empty() {
            eprintln!("Check the full log file: {}", self.log_file_path.display());
        }
    }

    /// Runs the garbage collector to delete old log files.
    ///
    /// A log file is a file in the [`log_dir`] and it will be deleted if its
    /// older than [`max_age`] days.
    ///
    /// [`log_dir`]: LoggerCore::log_dir
    /// [`max_age`]: LoggerCore::max_age
    async fn collect_garbage(&mut self) {
        if self.max_age == 0 {
            return;
        }

        let now = std::time::SystemTime::now();

        let Ok(mut logs) = fs::read_dir(&self.log_dir).await else {
            self.log(LogMessage {
                level: LogLevel::Error,
                message: "Failed to read the logs directory during garbage collection".into(),
            })
            .await;
            return;
        };

        loop {
            let log = logs.next_entry().await;
            let Ok(log) = log else {
                continue;
            };

            let Some(log) = log else {
                break;
            };

            let filename = log.file_name();

            if !filename.to_string_lossy().ends_with(".log")
                || !filename.to_string_lossy().starts_with("patch-hub_")
            {
                continue;
            }

            let Ok(Ok(created_date)) = log.metadata().await.map(|meta| meta.created()) else {
                continue;
            };
            let Ok(age) = now.duration_since(created_date) else {
                continue;
            };
            let age = age.as_secs() / 60 / 60 / 24;

            if age as usize > self.max_age && std::fs::remove_file(log.path()).is_err() {
                self.log(LogMessage {
                    message: format!(
                        "Failed to remove the log file: {}",
                        log.path().to_string_lossy()
                    ),
                    level: LogLevel::Warning,
                })
                .await;
            }
        }
    }
}

/// The possible commands to be handled by the logger actor. They will be
/// executed synchronously in the same order that they were sent through the
/// channel
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Logs the payload message
    Log(LogMessage),
    /// Flushes the logger by closing the log file, printing critical errors to
    /// the stdout and destroying the logger instance
    Flush,
    /// Runs the log garbage collector deleting old files according with the
    /// configured in the logger
    CollectGarbage,
}

/// The transmitter that sends messages down to a logger actor. This is what
/// you're supossed to use accross the code to log messages, not LoggerCore.
/// Cloning it is cheap so do not feel afraid to pass it around.
///
/// The transmitter is obtained by calling [`spawn`] on a [`LoggerCore`]
/// instance, consuming it and creating a dedicated task for it. Use the methods
/// of this struct to interact with the logger.
///
/// The intended usage is:
/// - Instantiate the logger with [`LoggerCore::build`]
/// - Spawn the logger actor with [`LoggerCore::spawn`]
/// - Use the methods of this struct to log messages
/// - Use the method [`flush`] to print the log messages to [`stderr`]
///     and finish the logger
///
/// [`spawn`]: LoggerCore::spawn
/// [`flush`]: Logger::flush
/// [`stderr`]: std::io::stderr
#[derive(Debug, Clone)]
pub enum Logger {
    /// The default version (produced by [`LoggerCore::spawn`])
    Default(Sender<Command>),
    /// The mock version of this logger which won't do nothing at all
    #[allow(dead_code)]
    Mock,
}

impl From<LoggerCore> for Logger {
    fn from(value: LoggerCore) -> Self {
        value.spawn().0
    }
}

impl Logger {
    /// Helper to simplify the logging process. This method sends a
    /// [`LogMessage`] to the logger. Will send the message in a new task so it
    /// won't block the caller
    ///
    /// # Panics
    /// If the logger was flushed
    fn log(&self, message: String, level: LogLevel) {
        let sender = match self {
            Logger::Mock => return,
            Logger::Default(sender) => sender.clone(),
        };

        tokio::spawn(async move {
            sender
                .send(Command::Log(LogMessage {
                    level,
                    message: message.to_string(),
                }))
                .await
                .expect("Attemp to use logger after a flush");
        });
    }

    /// Log a message with the `INFO` level
    ///
    /// # Panics
    /// If the logger was flushed
    pub fn info<M: Display>(&self, message: M) {
        self.log(message.to_string(), LogLevel::Info);
    }

    /// Log a message with the `WARNING` level
    ///
    /// # Panics
    /// If the logger was flushed
    pub fn warn<M: Display>(&self, message: M) {
        self.log(message.to_string(), LogLevel::Warning);
    }

    /// Log a message with the `ERROR` level
    pub fn error<M: Display>(&self, message: M) {
        self.log(message.to_string(), LogLevel::Error);
    }

    /// Log an info message if the result is an error
    /// and return the result as is
    ///
    /// # Panics
    /// If the logger was flushed
    #[allow(dead_code)]
    pub fn info_on_error<T, E: Display>(&self, result: Result<T, E>) -> Result<T, E> {
        match result {
            Ok(value) => Ok(value),
            Err(err) => {
                self.log(err.to_string(), LogLevel::Info);
                Err(err)
            }
        }
    }

    /// Log an warning message if the result is an error
    /// and return the result as is
    ///
    /// # Panics
    /// If the logger was flushed
    #[allow(dead_code)]
    pub fn warn_on_error<T, E: Display>(&self, result: Result<T, E>) -> Result<T, E> {
        match result {
            Ok(value) => Ok(value),
            Err(err) => {
                self.log(err.to_string(), LogLevel::Warning);
                Err(err)
            }
        }
    }

    /// Log an error message if the result is an error
    /// and return the result as is
    ///
    /// # Panics
    /// If the logger was flushed
    pub fn error_on_error<T, E: Display>(&self, result: Result<T, E>) -> Result<T, E> {
        match result {
            Ok(value) => Ok(value),
            Err(err) => {
                self.log(err.to_string(), LogLevel::Error);
                Err(err)
            }
        }
    }

    /// Flushes the logger by printing its messages to [`stderr`] and closing
    /// the log file. After this method is called, the logger is destroyed and
    /// any attempt to use it will panic.
    ///
    /// # Panics
    /// If called twice
    ///
    /// [`stderr`]: std::io::stderr
    pub fn flush(self) -> JoinHandle<()> {
        let Self::Default(sender) = self else {
            return tokio::spawn(async {});
        };

        tokio::spawn(async move {
            sender
                .send(Command::Flush)
                .await
                .expect("Flushing a logger twice");
        })
    }

    /// Collects the garbage from the logs directory. Garbage logs are the ones
    /// older than the [`max_age`] set during the logger [`build`].
    ///
    /// # Panics
    /// If called after a flush
    ///
    /// [`build`]: Logger::build
    /// [`max_age`]: Logger::max_age
    pub async fn collect_garbage(&self) {
        let Self::Default(sender) = self else {
            return;
        };

        sender
            .send(Command::CollectGarbage)
            .await
            .expect("Attemp to use logger after a flush")
    }
}
