#[macro_export]
macro_rules! log_on_error {
    ($result:expr) => {
        log_on_error!($crate::app::logging::logger::LogLevel::Error, $result)
    };
    ($level:expr, $result:expr) => {
        match $result {
            Ok(_) => $result,
            Err(ref error) => {
                let error_message =
                    format!("Error executing {:?}: {}", stringify!($result), &error);
                match $level {
                    $crate::app::logging::logger::LogLevel::Info => {
                        Logger::info(error_message);
                    }
                    $crate::app::logging::logger::LogLevel::Warning => {
                        Logger::warn(error_message);
                    }
                    $crate::app::logging::logger::LogLevel::Error => {
                        Logger::error(error_message);
                    }
                }
                $result
            }
        }
    };
}
