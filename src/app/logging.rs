use std::{
    fs::{self, File, OpenOptions},
    io::Write,
};

use super::config::Config;

static mut LOG_BUFFER: Logger = Logger {
    buffer: Vec::new(),
    log_file: None,
    log_filepath: None,
};

/// The Logger singleton that manages logging to [`stderr`] (log buffer) and a log file. 
/// This is safe to use only in single-threaded scenarios. The messages are written to the log file immediatly, 
/// but the messages to the `stderr` are written only after the TUI is closed, so they are kept in memory.
///
/// The expected flow is:
///  - Initialize the log file with [`init_log_file`]
///  - Write to the log file with [`write`]
///  - Flush the log buffer to the stderr and close the log file with [`flush`]
///
/// The log file is created in the logs_path defined in the [`Config`] struct
///
/// [`Config`]: super::config::Config
/// [`init_log_file`]: Logger::init_log_file
/// [`write`]: Logger::write
/// [`flush`]: Logger::flush
/// [`stderr`]: std::io::stderr
#[derive(Debug)]
pub struct Logger {
    buffer: Vec<String>,
    log_file: Option<File>,
    log_filepath: Option<String>,
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
    pub fn write(&mut self, msg: &str) {
        let msg = format!(
            "[{}] {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            msg
        );

        let file = self.log_file.as_mut().expect("Log file not initialized, make sure to call Logger::init_log_file() before writing to the log file");
        writeln!(file, "{msg}")
            .expect("Failed to write to log file");

        self.buffer.push(msg);
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
        for line in &self.buffer {
            eprintln!("{}", line);
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
