use color_eyre::{config::HookBuilder, eyre};
use ratatui::layout::Position;
use std::io::{self, stdout, Stdout};
use std::panic;

use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    prelude::Backend,
    Terminal,
};

use crate::logger::{LoggerActor, LoggerTx};

/// A type alias for the terminal type used in this application
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Initialize the terminal
pub fn init() -> io::Result<Tui> {
    execute!(stdout(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    Terminal::new(CrosstermBackend::new(stdout()))
}

/// Restore the terminal to its original state
pub fn restore() -> io::Result<()> {
    execute!(stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

/// This replaces the standard color_eyre panic and error hooks with hooks that
/// restore the terminal before printing the panic or error.
pub fn install_hooks(logger: LoggerTx) -> color_eyre::Result<()> {
    let (panic_hook, eyre_hook) = HookBuilder::default().into_hooks();

    // convert from a color_eyre PanicHook to a standard panic hook
    let panic_hook = panic_hook.into_panic_hook();
    panic::set_hook(Box::new(move |panic_info| {
        restore().unwrap();
        // TODO: await for the flush to finish
        <LoggerTx as Clone>::clone(&logger).flush();

        panic_hook(panic_info);
    }));

    // convert from a color_eyre EyreHook to a eyre ErrorHook
    let eyre_hook = eyre_hook.into_eyre_hook();
    eyre::set_hook(Box::new(
        move |error: &(dyn std::error::Error + 'static)| {
            restore().unwrap();
            eyre_hook(error)
        },
    ))?;

    Ok(())
}

pub fn setup_user_io<B: Backend>(terminal: &mut Terminal<B>) -> color_eyre::Result<()> {
    terminal.clear()?;
    terminal.set_cursor_position(Position::new(0, 0))?;
    terminal.show_cursor()?;
    disable_raw_mode()?;
    Ok(())
}

pub fn teardown_user_io<B: Backend>(terminal: &mut Terminal<B>) -> color_eyre::Result<()> {
    enable_raw_mode()?;
    terminal.clear()?;
    Ok(())
}

#[inline]
/// Simply calls `which` to check if a binary exists
pub fn binary_exists(binary: &str) -> bool {
    which::which(binary).is_ok()
}

#[macro_export]
/// Macro that encapsulates a piece of code that takes long to run and displays a loading screen while it runs.
///
/// This macro takes two arguments: the terminal and the title of the loading screen (anything that implements `Display`).
/// After a `=>` token, you can pass the code that takes long to run.
///
/// When the execution finishes, the macro will return the terminal.
///
/// Important to notice that the code block will run in the same scope as the rest of the macro
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

            $inst;

            loading.store(false, std::sync::atomic::Ordering::Relaxed);

            handle.join().unwrap()
        }
    };
}

#[cfg(test)]
mod tests {
    use std::sync::Once;

    use super::*;

    static INIT: Once = Once::new();

    // Tests can be run in parallel, we don't want to override previously installed hooks
    fn setup() {
        INIT.call_once(|| {
            install_hooks().expect("Failed to install hooks");
        })
    }

    #[test]
    fn test_binary_exists() {
        // cargo should always exist since we are running the tests with `cargo test`
        assert!(super::binary_exists("cargo"));
        // there is no way this binary exists
        assert!(!super::binary_exists("there_is_no_way_this_binary_exists"));
    }

    #[test]
    fn test_install_hooks() {
        setup();
    }

    #[test]
    fn test_error_hook_works() {
        setup();

        let result: color_eyre::Result<()> = Err(eyre::eyre!("Test error"));

        // We can't directly test the hook's formatting, but we can verify
        // that handling an error doesn't cause unexpected panics
        match result {
            Ok(_) => panic!("Expected an error"),
            Err(e) => {
                let _ = format!("{:?}", e);
            }
        }
    }

    #[test]
    fn test_panic_hook() {
        setup();

        let result = std::panic::catch_unwind(|| std::panic!("Test panic"));

        assert!(result.is_err());
    }
}
