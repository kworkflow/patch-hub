//! `UiCore` — the stateful presentation layer.
//!
//! `UiCore` transforms an [`AppViewModel`] into a paint-ready [`UiScene`] by
//! dispatching to per-screen builders and composing the navigation bar and
//! optional popup. It holds a [`UiTheme`] so future styling policy can be
//! applied centrally without touching individual screen painters.

use crate::app::view_model::{AppViewModel, ScreenViewModel};
use crate::ui::{
    errors::UiError,
    scene::{NavigationBarScene, UiBody, UiScene},
    screens,
    theme::UiTheme,
};

pub struct UiCore {
    #[allow(dead_code)]
    theme: UiTheme,
}

impl UiCore {
    pub fn new() -> Self {
        Self { theme: UiTheme }
    }

    /// Transform `vm` into a fully projected [`UiScene`] ready for painting.
    pub fn build_scene(&self, vm: &AppViewModel) -> Result<UiScene, UiError> {
        let body = match &vm.screen {
            ScreenViewModel::MailingListSelection(mls_vm) => {
                UiBody::MailingListSelection(screens::mailing_list::build_scene(mls_vm))
            }
            ScreenViewModel::Bookmarked(b_vm) => {
                UiBody::Bookmarked(screens::bookmarked::build_scene(b_vm))
            }
            ScreenViewModel::Latest(l_vm) => UiBody::Latest(screens::latest::build_scene(l_vm)),
            ScreenViewModel::PatchsetDetails(pd_vm) => {
                UiBody::PatchsetDetails(screens::details::build_scene(pd_vm))
            }
            ScreenViewModel::EditConfig(ec_vm) => {
                UiBody::EditConfig(screens::edit_config::build_scene(ec_vm))
            }
        };

        let navigation = self.build_navigation(&vm.screen);
        let popup = vm.popup.as_ref().map(screens::popup::build_scene);

        Ok(UiScene {
            body,
            navigation,
            popup,
        })
    }

    fn build_navigation(&self, screen: &ScreenViewModel) -> NavigationBarScene {
        let (mode_spans, keys_hint) = match screen {
            ScreenViewModel::MailingListSelection(vm) => (
                screens::mailing_list::mode_spans(vm),
                screens::mailing_list::keys_hint_span(),
            ),
            ScreenViewModel::Bookmarked(_) => (
                screens::bookmarked::mode_spans(),
                screens::bookmarked::keys_hint_span(),
            ),
            ScreenViewModel::Latest(vm) => (
                screens::latest::mode_spans(vm),
                screens::latest::keys_hint_span(),
            ),
            ScreenViewModel::PatchsetDetails(_) => (
                screens::details::mode_spans(),
                screens::details::keys_hint_span(),
            ),
            ScreenViewModel::EditConfig(vm) => (
                screens::edit_config::mode_spans(vm),
                screens::edit_config::keys_hint_span(vm),
            ),
        };
        NavigationBarScene {
            mode_spans,
            keys_hint,
        }
    }
}

impl Default for UiCore {
    fn default() -> Self {
        Self::new()
    }
}
