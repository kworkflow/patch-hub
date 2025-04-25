use std::panic;

use super::{logging::Logger, terminal::restore};

/// This replaces the standard color_eyre panic and error hooks with hooks that
/// restore the terminal before printing the panic or error.
pub fn install_hooks() -> color_eyre::Result<()> {
    let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default().into_hooks();

    // convert from a color_eyre PanicHook to a standard panic hook
    let panic_hook = panic_hook.into_panic_hook();
    panic::set_hook(Box::new(move |panic_info| {
        restore().unwrap();
        Logger::flush();
        panic_hook(panic_info);
    }));

    // convert from a color_eyre EyreHook to a eyre ErrorHook
    let eyre_hook = eyre_hook.into_eyre_hook();
    color_eyre::eyre::set_hook(Box::new(
        move |error: &(dyn std::error::Error + 'static)| {
            restore().unwrap();
            eyre_hook(error)
        },
    ))?;

    Ok(())
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
    fn test_install_hooks() {
        setup();
    }

    #[test]
    fn test_error_hook_works() {
        setup();

        let result: color_eyre::Result<()> = Err(color_eyre::eyre::eyre!("Test error"));

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
