use super::{EnvTrait, OsEnv};

#[test]
fn var_returns_existing_variable() {
    let env = OsEnv;
    // PATH is universally set
    let result = env.var("PATH");
    assert!(result.is_ok(), "PATH should be set");
    assert!(!result.unwrap().is_empty());
}

#[test]
fn var_returns_error_for_missing_variable() {
    let env = OsEnv;
    let result = env.var("__PATCH_HUB_NONEXISTENT_VAR__");
    assert!(result.is_err());
}

#[test]
fn which_returns_true_for_installed_binary() {
    let env = OsEnv;
    // sh is present on every POSIX system
    assert!(env.which("sh"));
}

#[test]
fn which_returns_false_for_missing_binary() {
    let env = OsEnv;
    assert!(!env.which("__nonexistent_binary_patch_hub__"));
}
