use std::ops::ControlFlow;

use crate::{app::screens::CurrentScreen, input::event::InputEvent};

use super::helpers::{
    app_harness::{dummy_terminal_handle, AppHarness},
    loading::FakeLoadingIndicator,
    lore::lore_handle_with_successful_patch_flow,
    render::render_handle_with_successful_preview,
};

#[tokio::test]
async fn latest_list_opens_details_and_back_returns_to_latest() {
    let mut harness = AppHarness::with_handles(
        lore_handle_with_successful_patch_flow(),
        render_handle_with_successful_preview(),
    );
    let mut loading = FakeLoadingIndicator::default();

    let result = harness
        .app
        .handle_mailing_list_selection(InputEvent::OpenLatestPatchsets, &mut loading)
        .await
        .expect("mailing list selection handles");
    assert_eq!(ControlFlow::Continue(()), result);
    assert_eq!(
        CurrentScreen::LatestPatchsets,
        harness.app.state.navigation.current_screen
    );

    harness
        .app
        .handle_latest_patchsets(InputEvent::OpenPatchsetDetails, &mut loading)
        .await
        .expect("latest patchsets handles");
    assert_eq!(
        CurrentScreen::PatchsetDetails,
        harness.app.state.navigation.current_screen
    );
    assert!(harness.app.state.lore.details.is_some());

    harness
        .app
        .handle_patchset_details(InputEvent::Back, &dummy_terminal_handle())
        .await
        .expect("patchset details handles");

    assert_eq!(
        CurrentScreen::LatestPatchsets,
        harness.app.state.navigation.current_screen
    );
    assert!(harness.app.state.lore.details.is_none());
}

#[tokio::test]
async fn bookmarked_list_opens_details_and_back_returns_to_bookmarks() {
    let mut harness = AppHarness::with_bookmark(
        lore_handle_with_successful_patch_flow(),
        render_handle_with_successful_preview(),
    );
    let mut loading = FakeLoadingIndicator::default();

    let result = harness
        .app
        .handle_mailing_list_selection(InputEvent::OpenBookmarkedPatchsets, &mut loading)
        .await
        .expect("mailing list selection handles");
    assert_eq!(ControlFlow::Continue(()), result);
    assert_eq!(
        CurrentScreen::BookmarkedPatchsets,
        harness.app.state.navigation.current_screen
    );

    harness
        .app
        .handle_bookmarked_patchsets(InputEvent::OpenPatchsetDetails, &mut loading)
        .await
        .expect("bookmarked patchsets handles");
    assert_eq!(
        CurrentScreen::PatchsetDetails,
        harness.app.state.navigation.current_screen
    );
    assert!(harness.app.state.lore.details.is_some());

    harness
        .app
        .handle_patchset_details(InputEvent::Back, &dummy_terminal_handle())
        .await
        .expect("patchset details handles");

    assert_eq!(
        CurrentScreen::BookmarkedPatchsets,
        harness.app.state.navigation.current_screen
    );
    assert!(harness.app.state.lore.details.is_none());
}
