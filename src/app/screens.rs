pub mod bookmarked;
pub mod details;
pub mod edit_config;
pub mod latest;
pub mod mail_list;

#[derive(Debug, Clone, PartialEq)]
pub enum CurrentScreen {
    MailingListSelection,
    BookmarkedPatchsets,
    LatestPatchsets,
    PatchsetDetails,
    EditConfig,
}
