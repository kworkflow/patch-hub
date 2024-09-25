use color_eyre::eyre::bail;
use patch_hub::{lore_api_client::BlockingLoreAPIClient, lore_session, mailing_list::MailingList};

pub struct MailingListSelectionState {
    pub mailing_lists: Vec<MailingList>,
    pub target_list: String,
    pub possible_mailing_lists: Vec<MailingList>,
    pub highlighted_list_index: usize,
    pub mailing_lists_path: String,
}

impl MailingListSelectionState {
    pub fn refresh_available_mailing_lists(&mut self) -> color_eyre::Result<()> {
        let lore_api_client = BlockingLoreAPIClient::new();

        match lore_session::fetch_available_lists(&lore_api_client) {
            Ok(available_mailing_lists) => {
                self.mailing_lists = available_mailing_lists;
            }
            Err(failed_available_lists_request) => {
                bail!(format!("{failed_available_lists_request:#?}"));
            }
        };

        self.clear_target_list();

        lore_session::save_available_lists(&self.mailing_lists, &self.mailing_lists_path)?;

        Ok(())
    }

    pub fn remove_last_target_list_char(&mut self) {
        if !self.target_list.is_empty() {
            self.target_list.pop();
            self.process_possible_mailing_lists();
        }
    }

    pub fn push_char_to_target_list(&mut self, ch: char) {
        self.target_list.push(ch);
        self.process_possible_mailing_lists();
    }

    pub fn clear_target_list(&mut self) {
        self.target_list.clear();
        self.process_possible_mailing_lists();
    }

    fn process_possible_mailing_lists(&mut self) {
        let mut possible_mailing_lists: Vec<MailingList> = Vec::new();

        for mailing_list in &self.mailing_lists {
            if mailing_list.get_name().starts_with(&self.target_list) {
                possible_mailing_lists.push(mailing_list.clone());
            }
        }

        self.possible_mailing_lists = possible_mailing_lists;
        self.highlighted_list_index = 0;
    }

    pub fn highlight_below_list(&mut self) {
        if self.highlighted_list_index + 1 < self.possible_mailing_lists.len() {
            self.highlighted_list_index += 1;
        }
    }

    pub fn highlight_above_list(&mut self) {
        self.highlighted_list_index = self.highlighted_list_index.saturating_sub(1);
    }

    pub fn has_valid_target_list(&self) -> bool {
        let list_length = self.possible_mailing_lists.len(); // Possible mailing list length
        let list_index = self.highlighted_list_index; // Index of the selected mailing list

        if list_index < list_length {
            return true;
        }
        false
    }
}
