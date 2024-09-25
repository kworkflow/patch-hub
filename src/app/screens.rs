pub mod bookmarked;
pub mod details;
pub mod latest;
pub mod mail_list;
pub mod edit_config;

#[derive(Debug, Clone, PartialEq)]
pub enum CurrentScreen {
    MailingListSelection,
    BookmarkedPatchsets,
    LatestPatchsets,
    PatchsetDetails,
    EditConfig,
}
