#[macro_export]
/// Macro that encapsulates a piece of code that takes long to run and displays a loading screen while it runs.
///
/// This macro takes two arguments: the terminal and the title of the loading screen (anything that implements `Display`).
/// After a `=>` token, you can pass the code that takes long to run.
///
/// When the execution finishes, the macro will return the terminal.
///
/// Important to notice that the code block will run in the same scope as the rest of the macro.
/// Be aware that in Rust, when using `?` or `return` inside a closure, they apply to the outer function,
/// not the closure itself. This can lead to unexpected behavior if you expect the closure to handle
/// errors or return values independently of the enclosing function.
///
/// # Example
/// ```rust norun
/// terminal = loading_screen! { terminal, "Loading stuff" => {
///    // code that takes long to run
/// }};
/// ```
macro_rules! loading_screen {
    { $terminal:expr, $title:expr => $inst:expr} => {
        {
            let loading = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let loading_clone = std::sync::Arc::clone(&loading);
            let mut terminal = $terminal;

            let handle = std::thread::spawn(move || {
                while loading_clone.load(std::sync::atomic::Ordering::Relaxed) {
                    terminal = $crate::ui::loading_screen::render(terminal, $title);
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }

                terminal
            });

            // we have to sleep so the loading thread completes at least one render
            std::thread::sleep(std::time::Duration::from_millis(200));
            let inst_result = $inst;

            loading.store(false, std::sync::atomic::Ordering::Relaxed);

            let terminal = handle.join().unwrap();

            inst_result?;

            terminal
        }
    };
}

#[macro_export]
macro_rules! log_on_error {
    ($result:expr) => {
        log_on_error!($crate::infrastructure::logging::LogLevel::Error, $result)
    };
    ($level:expr, $result:expr) => {
        match $result {
            Ok(_) => $result,
            Err(ref error) => {
                let error_message =
                    format!("Error executing {:?}: {}", stringify!($result), &error);
                match $level {
                    $crate::infrastructure::logging::LogLevel::Info => {
                        Logger::info(error_message);
                    }
                    $crate::infrastructure::logging::LogLevel::Warning => {
                        Logger::warn(error_message);
                    }
                    $crate::infrastructure::logging::LogLevel::Error => {
                        Logger::error(error_message);
                    }
                }
                $result
            }
        }
    };
}
