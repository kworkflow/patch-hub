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

pub(crate) struct TerminalRestoreGuard {
    started: AtomicBool,
}

static TERMINAL_RESTORE: TerminalRestoreGuard = TerminalRestoreGuard::new();

impl TerminalRestoreGuard {
    /// Restores the terminal at most once in this process.
    ///
    /// The panic hook and `main`'s fatal-error return both call this. A panic in a
    /// spawned task runs the hook and then comes back to `main` as a join error,
    /// so those two calls would otherwise restore twice.
    pub(crate) fn restore_once() {
        TERMINAL_RESTORE.call(restore);
    }
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

/// Installs the panic and eyre hooks. The panic hook restores the terminal,
/// then color_eyre prints. The eyre hook only forwards to color_eyre: it
/// runs for every report, including handled errors, so it must not restore.
/// Fatal `main` errors call `TerminalRestoreGuard::restore_once` before the
/// report prints; normal shutdown uses `TerminalHandle::shutdown`.
pub fn install_hooks() -> Result<()> {
    let (panic_hook, eyre_hook) = HookBuilder::default().into_hooks();

    // convert from a color_eyre PanicHook to a standard panic hook
    let panic_hook = panic_hook.into_panic_hook();
    panic::set_hook(Box::new(move |panic_info| {
        TerminalRestoreGuard::restore_once();
        panic_hook(panic_info);
    }));

    // convert from a color_eyre EyreHook to a eyre ErrorHook
    let eyre_hook = eyre_hook.into_eyre_hook();
    set_hook(Box::new(move |error: &(dyn Error + 'static)| {
        eyre_hook(error)
    }))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use color_eyre::eyre;
    use std::panic::catch_unwind;

    #[test]
    fn test_error_hook_works() {
        let result: color_eyre::Result<()> = Err(eyre::eyre!("Test error"));

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
        let result = catch_unwind(|| std::panic!("Test panic"));

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
