use crate::lore::mailing_list::MailingList;

pub struct MailingListsState {
    pub highlighted_list_index: usize,
    pub filtered_mailing_lists: Vec<MailingList>,
    pub mailing_lists: Vec<MailingList>,
    pub search_string: String,
}
