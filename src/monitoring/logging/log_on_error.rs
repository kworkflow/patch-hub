#[macro_export]
macro_rules! log_on_error {
    ($result:expr) => {
        log_on_error!(tracing::Level::ERROR, $result)
    };
    ($level:expr, $result:expr) => {
        match $result {
            Ok(_) => $result,
            Err(ref error) => {
                let error_message =
                    format!("Error executing {:?}: {}", stringify!($result), &error);
                tracing::event!($level, error_message);
                $result
            }
        }
    };
}
