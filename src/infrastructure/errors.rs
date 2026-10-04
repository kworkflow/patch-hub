use std::{
    error::Error,
    io::{self, Write},
    panic,
    sync::atomic::{AtomicBool, Ordering},
};

use color_eyre::{
    config::HookBuilder,
    eyre::{set_hook, Result},
};

use super::terminal::restore;

struct TerminalRestoreGuard {
    started: AtomicBool,
}

impl TerminalRestoreGuard {
    const fn new() -> Self {
        Self {
            started: AtomicBool::new(false),
        }
    }

    fn call(&self, restore: impl FnOnce() -> io::Result<()>) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Err(err) = restore() {
            let _ = writeln!(io::stderr(), "failed to restore terminal: {err}");
        }
    }
}

static TERMINAL_RESTORE: TerminalRestoreGuard = TerminalRestoreGuard::new();

/// This replaces the standard color_eyre panic and error hooks with hooks that
/// restore the terminal before printing the panic or error.
///
/// Normal application shutdown restores the terminal through
/// [`crate::terminal::handle::TerminalHandle::shutdown`]. These hooks keep a
/// direct [`super::terminal::restore`] fallback for panics and fatal errors.
pub fn install_hooks() -> Result<()> {
    let (panic_hook, eyre_hook) = HookBuilder::default().into_hooks();

    // convert from a color_eyre PanicHook to a standard panic hook
    let panic_hook = panic_hook.into_panic_hook();
    panic::set_hook(Box::new(move |panic_info| {
        TERMINAL_RESTORE.call(restore);
        panic_hook(panic_info);
    }));

    // convert from a color_eyre EyreHook to a eyre ErrorHook
    let eyre_hook = eyre_hook.into_eyre_hook();
    set_hook(Box::new(move |error: &(dyn Error + 'static)| {
        TERMINAL_RESTORE.call(restore);
        eyre_hook(error)
    }))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    #[test]
    fn test_error_hook_works() {
        let result: color_eyre::Result<()> = Err(color_eyre::eyre::eyre!("Test error"));

        // We can't directly test the hook's formatting, but we can verify
        // that handling an error doesn't cause unexpected panics
        match result {
            Ok(_) => panic!("Expected an error"),
            Err(e) => {
                let _ = format!("{e:?}");
            }
        }
    }

    #[test]
    fn test_panic_hook() {
        let result = std::panic::catch_unwind(|| std::panic!("Test panic"));

        assert!(result.is_err());
    }

    #[test]
    fn restore_guard_runs_the_operation_once() {
        let guard = TerminalRestoreGuard::new();
        let calls = AtomicUsize::new(0);
        let attempt = || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        };

        guard.call(attempt);
        guard.call(attempt);

        assert_eq!(1, calls.load(Ordering::SeqCst));
    }
}
