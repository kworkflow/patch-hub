use crate::app::{popup::AppPopup, screens::CurrentScreen, App, B4Result};

#[derive(Debug, PartialEq)]
enum OpenPatchsetAction {
    ShowDetails,
    ShowError { origin: CurrentScreen, body: String },
}

pub(super) fn apply_open_patchset_result(
    app: &mut App,
    origin: CurrentScreen,
    result: color_eyre::Result<B4Result>,
) {
    match resolve_open_patchset_result(origin, result) {
        OpenPatchsetAction::ShowDetails => {
            app.set_current_screen(CurrentScreen::PatchsetDetails);
        }
        OpenPatchsetAction::ShowError { origin, body } => {
            app.state.popup = Some(AppPopup::info("Error", body));
            app.set_current_screen(origin);
        }
    }
}

fn resolve_open_patchset_result(
    origin: CurrentScreen,
    result: color_eyre::Result<B4Result>,
) -> OpenPatchsetAction {
    match result {
        Ok(B4Result::PatchFound) => OpenPatchsetAction::ShowDetails,
        Ok(B4Result::PatchNotFound(err_cause)) => OpenPatchsetAction::ShowError {
            origin,
            body: format!(
                "The selected patchset couldn't be retrieved.\nReason: {err_cause}\nPlease choose another patchset."
            ),
        },
        Err(error) => OpenPatchsetAction::ShowError {
            origin,
            body: format!("The selected patchset couldn't be opened.\nReason: {error:#}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use color_eyre::eyre::eyre;

    use super::*;

    #[test]
    fn patch_found_opens_details() {
        let action =
            resolve_open_patchset_result(CurrentScreen::LatestPatchsets, Ok(B4Result::PatchFound));

        assert_eq!(OpenPatchsetAction::ShowDetails, action);
    }

    #[test]
    fn patch_not_found_shows_retrieval_error_on_origin_screen() {
        let action = resolve_open_patchset_result(
            CurrentScreen::BookmarkedPatchsets,
            Ok(B4Result::PatchNotFound("not found by b4".to_string())),
        );

        assert_eq!(
            OpenPatchsetAction::ShowError {
                origin: CurrentScreen::BookmarkedPatchsets,
                body: "The selected patchset couldn't be retrieved.\nReason: not found by b4\nPlease choose another patchset.".to_string(),
            },
            action,
        );
    }

    #[test]
    fn unexpected_error_shows_open_error_on_origin_screen() {
        let action = resolve_open_patchset_result(
            CurrentScreen::LatestPatchsets,
            Err(eyre!("render actor unavailable")),
        );

        let OpenPatchsetAction::ShowError { origin, body } = action else {
            panic!("unexpected errors should show an error popup");
        };
        assert_eq!(CurrentScreen::LatestPatchsets, origin);
        assert!(body.contains("The selected patchset couldn't be opened."));
        assert!(body.contains("render actor unavailable"));
    }
}
