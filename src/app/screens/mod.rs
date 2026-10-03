pub mod bookmarked;
pub mod details_actions;
pub mod edit_config;
pub mod kw_ops;
pub mod latest;
pub mod mail_list;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum CurrentScreen {
    #[default]
    MailingListSelection,
    BookmarkedPatchsets,
    LatestPatchsets,
    PatchsetDetails,
    EditConfig,
    KwOps,
}
