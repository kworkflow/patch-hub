use std::ops::ControlFlow;

use crate::{
    app::{models::popup::AppPopup, screens::CurrentScreen},
    input::event::InputEvent,
};

use super::helpers::{
    app_harness::AppHarness,
    loading::FakeLoadingIndicator,
    lore::{lore_handle_with_patch_details_failure, lore_handle_with_successful_patch_flow},
    render::{render_handle_with_preview_failure, render_handle_with_successful_preview},
};

#[tokio::test]
async fn latest_render_failure_shows_popup_and_stays_on_latest() {
    let mut harness = AppHarness::with_handles(
        lore_handle_with_successful_patch_flow(),
        render_handle_with_preview_failure(),
    );
    let mut loading = FakeLoadingIndicator::default();

    let result = harness
        .app
        .handle_mailing_list_selection(InputEvent::OpenLatestPatchsets, &mut loading)
        .await
        .expect("mailing list selection handles");
    assert_eq!(ControlFlow::Continue(()), result);

    harness
        .app
        .handle_latest_patchsets(InputEvent::OpenPatchsetDetails, &mut loading)
        .await
        .expect("latest patchsets handles");

    assert_eq!(
        CurrentScreen::LatestPatchsets,
        harness.app.state.navigation.current_screen
    );
    assert!(harness.app.state.lore.details.is_none());
    assert_info_popup_contains(
        harness.app.state.popup.as_ref(),
        "Error",
        &[
            "The selected patchset couldn't be opened.",
            "render actor unavailable in test",
        ],
    );
}

#[tokio::test]
async fn bookmarked_lore_failure_shows_popup_and_stays_on_bookmarks() {
    let mut harness = AppHarness::with_bookmark(
        lore_handle_with_patch_details_failure(),
        render_handle_with_successful_preview(),
    );
    let mut loading = FakeLoadingIndicator::default();

    let result = harness
        .app
        .handle_mailing_list_selection(InputEvent::OpenBookmarkedPatchsets, &mut loading)
        .await
        .expect("mailing list selection handles");
    assert_eq!(ControlFlow::Continue(()), result);

    harness
        .app
        .handle_bookmarked_patchsets(InputEvent::OpenPatchsetDetails, &mut loading)
        .await
        .expect("bookmarked patchsets handles");

    assert_eq!(
        CurrentScreen::BookmarkedPatchsets,
        harness.app.state.navigation.current_screen
    );
    assert!(harness.app.state.lore.details.is_none());
    assert_info_popup_contains(
        harness.app.state.popup.as_ref(),
        "Error",
        &[
            "The selected patchset couldn't be opened.",
            "lore actor unavailable in test",
        ],
    );
}

fn assert_info_popup_contains(popup: Option<&AppPopup>, expected_title: &str, expected: &[&str]) {
    let Some(AppPopup::Info { title, body, .. }) = popup else {
        panic!("expected info popup");
    };

    assert_eq!(expected_title, title);
    for fragment in expected {
        assert!(
            body.contains(fragment),
            "expected popup body to contain {fragment:?}, got {body:?}"
        );
    }
}
