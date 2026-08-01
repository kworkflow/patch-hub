use std::ops::ControlFlow;

use crate::{
    app::{
        flows::{
            bookmarked::handle_bookmarked_patchsets, details_actions::handle_patchset_details,
            latest::handle_latest_patchsets, mail_list::handle_mailing_list_selection,
        },
        screens::CurrentScreen,
    },
    input::event::InputEvent,
};

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

    let result = handle_mailing_list_selection(
        &mut harness.app,
        InputEvent::OpenLatestPatchsets,
        &mut loading,
    )
    .await
    .unwrap();
    assert_eq!(ControlFlow::Continue(()), result);
    assert_eq!(
        CurrentScreen::LatestPatchsets,
        harness.app.state.navigation.current_screen
    );

    handle_latest_patchsets(
        &mut harness.app,
        InputEvent::OpenPatchsetDetails,
        &mut loading,
    )
    .await
    .unwrap();
    assert_eq!(
        CurrentScreen::PatchsetDetails,
        harness.app.state.navigation.current_screen
    );
    assert!(harness.app.state.lore.details.is_some());

    handle_patchset_details(&mut harness.app, InputEvent::Back, &dummy_terminal_handle())
        .await
        .unwrap();

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

    let result = handle_mailing_list_selection(
        &mut harness.app,
        InputEvent::OpenBookmarkedPatchsets,
        &mut loading,
    )
    .await
    .unwrap();
    assert_eq!(ControlFlow::Continue(()), result);
    assert_eq!(
        CurrentScreen::BookmarkedPatchsets,
        harness.app.state.navigation.current_screen
    );

    handle_bookmarked_patchsets(
        &mut harness.app,
        InputEvent::OpenPatchsetDetails,
        &mut loading,
    )
    .await
    .unwrap();
    assert_eq!(
        CurrentScreen::PatchsetDetails,
        harness.app.state.navigation.current_screen
    );
    assert!(harness.app.state.lore.details.is_some());

    handle_patchset_details(&mut harness.app, InputEvent::Back, &dummy_terminal_handle())
        .await
        .unwrap();

    assert_eq!(
        CurrentScreen::BookmarkedPatchsets,
        harness.app.state.navigation.current_screen
    );
    assert!(harness.app.state.lore.details.is_none());
}
