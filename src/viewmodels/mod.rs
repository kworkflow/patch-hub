pub mod mailing_list_viewmodel;

use ratatui::crossterm::event::KeyEvent;

use crate::viewmodels::mailing_list_viewmodel::MailingListsState;

#[allow(dead_code)]
pub trait ViewModel {
    fn handle_key(&self, event: KeyEvent);

    fn state(&self) -> ViewModelState;
}

#[allow(dead_code)]
pub enum ViewModelState {
    MailingLists(MailingListsState),
    BookmarkedPatchsets,
    LatestPatchsets,
    PatchsetDetails,
    EditConfig,
}
