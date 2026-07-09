mod helpers;

use crate::app::{loading::LoadingIndicator, screens::CurrentScreen};

use helpers::{
    app_harness::AppHarness,
    loading::FakeLoadingIndicator,
    lore::{sample_patch, sample_patchset_details},
    render::sample_rendered_preview,
};

#[test]
fn helper_builds_minimal_app_on_initial_screen() {
    let harness = AppHarness::new();

    assert_eq!(
        CurrentScreen::MailingListSelection,
        harness.app.state.navigation.current_screen
    );
}

#[test]
fn helpers_provide_sample_patchset_inputs() {
    let patch = sample_patch();
    let details = sample_patchset_details();
    let rendered = sample_rendered_preview();

    assert_eq!("[PATCH 1/1] test patch", patch.title());
    assert_eq!(1, details.raw_patches.len());
    assert_eq!(1, details.tag_summary.len());
    assert_eq!(1, rendered.entries.len());
}

#[test]
fn fake_loading_indicator_records_start_and_stop() {
    let mut loading = FakeLoadingIndicator::default();

    loading.start("Loading patchset".to_string());
    loading.stop().unwrap();

    assert_eq!(vec!["Loading patchset"], loading.starts);
    assert_eq!(1, loading.stop_count);
}
