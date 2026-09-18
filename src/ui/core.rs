//! `UiCore` — the stateful presentation layer.
//!
//! `UiCore` transforms an [`AppViewModel`] into a paint-ready [`UiScene`] by
//! dispatching to per-screen builders and composing the navigation bar and
//! optional popup.

use crate::app::view_model::{AppViewModel, ScreenViewModel};
use crate::ui::{
    errors::UiError,
    scene::{NavigationBarScene, UiBody, UiScene},
    screens,
};
use ratatui::{
    style::{Color, Style},
    text::Span,
};

pub struct UiCore;

impl UiCore {
    pub fn new() -> Self {
        Self
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
            ScreenViewModel::KwOps(kw_vm) => UiBody::KwOps(screens::kw_ops::build_scene(kw_vm)),
        };

        let navigation = self.build_navigation(&vm.screen, vm.kw_running.as_deref());
        let popup = vm.popup.as_ref().map(screens::popup::build_scene);

        Ok(UiScene {
            body,
            navigation,
            popup,
        })
    }

    fn build_navigation(
        &self,
        screen: &ScreenViewModel,
        kw_running: Option<&str>,
    ) -> NavigationBarScene {
        let (mut mode_spans, keys_hint) = match screen {
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
            ScreenViewModel::KwOps(vm) => (
                screens::kw_ops::mode_spans(),
                screens::kw_ops::keys_hint_span(vm.editing),
            ),
        };
        if let Some(indicator) = kw_running {
            mode_spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            mode_spans.push(Span::styled(
                indicator.to_string(),
                Style::default().fg(Color::Yellow),
            ));
        }
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

#[cfg(test)]
mod tests {
    use crate::app::view_model::{
        AppViewModel, MailingListSelectionViewModel, ScreenViewModel, TargetListStatus,
    };

    use super::UiCore;

    fn mailing_list_vm(kw_running: Option<String>) -> AppViewModel {
        AppViewModel {
            screen: ScreenViewModel::MailingListSelection(MailingListSelectionViewModel {
                entries: vec![],
                highlighted_index: 0,
                target_list: String::new(),
                target_list_status: TargetListStatus::Empty,
            }),
            popup: None,
            kw_running,
        }
    }

    fn nav_text(vm: AppViewModel) -> String {
        UiCore::new()
            .build_scene(&vm)
            .unwrap()
            .navigation
            .mode_spans
            .into_iter()
            .map(|span| span.content.to_string())
            .collect()
    }

    #[test]
    fn running_indicator_is_appended_to_the_nav_bar() {
        let text = nav_text(mailing_list_vm(Some("kw: building patchset-x".to_string())));
        assert!(text.contains("Target List:"));
        assert!(text.contains(" | kw: building patchset-x"));
    }

    #[test]
    fn idle_nav_bar_has_no_kw_indicator() {
        let text = nav_text(mailing_list_vm(None));
        assert!(text.contains("Target List:"));
        assert!(!text.contains("kw:"));
    }
}
