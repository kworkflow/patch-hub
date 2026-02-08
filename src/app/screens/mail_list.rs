use color_eyre::eyre::bail;
use patch_hub::lore::{
    lore_api_client::BlockingLoreAPIClient, lore_session, mailing_list::MailingList,
};

pub struct MailingListSelection {
    pub mailing_lists: Vec<MailingList>,
    pub target_list: String,
    pub possible_mailing_lists: Vec<MailingList>,
    pub highlighted_list_index: usize,
    pub mailing_lists_path: String,
    pub lore_api_client: BlockingLoreAPIClient,
}

impl MailingListSelection {
    pub fn refresh_available_mailing_lists(&mut self) -> color_eyre::Result<()> {
        match lore_session::fetch_available_lists(&self.lore_api_client) {
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
        if self.target_list.is_empty() {
            self.possible_mailing_lists = self.mailing_lists.clone();
            self.highlighted_list_index = 0;
            return;
        }

        let query = self.target_list.to_lowercase();
        let mut exact_matches: Vec<MailingList> = Vec::new();
        let mut partial_matches: Vec<MailingList> = Vec::new();

        for mailing_list in &self.mailing_lists {
            let name_lower = mailing_list.name().to_lowercase();
            let desc_lower = mailing_list.description().to_lowercase();

            // Check for exact match first (case-insensitive)
            if name_lower == query {
                exact_matches.push(mailing_list.clone());
            } else if name_lower.contains(&query) || desc_lower.contains(&query) {
                partial_matches.push(mailing_list.clone());
            }
        }

        // Prioritize exact matches by placing them first
        let mut all_matches = exact_matches;
        all_matches.append(&mut partial_matches);
        self.possible_mailing_lists = all_matches;
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
