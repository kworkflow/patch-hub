use color_eyre::eyre::bail;

use crate::{
    infrastructure::file_system::FileSystemTrait,
    lore::{lore_api_client::AvailableListsRequest, lore_session, mailing_list::MailingList},
};

pub struct MailingListSelection {
    pub mailing_lists: Vec<MailingList>,
    pub target_list: String,
    pub possible_mailing_lists: Vec<MailingList>,
    pub highlighted_list_index: usize,
    pub mailing_lists_path: String,
    pub lore_api_client: Box<dyn AvailableListsRequest>,
}

impl MailingListSelection {
    pub fn refresh_available_mailing_lists(
        &mut self,
        fs: &dyn FileSystemTrait,
    ) -> color_eyre::Result<()> {
        match lore_session::fetch_available_lists(&*self.lore_api_client) {
            Ok(available_mailing_lists) => {
                self.mailing_lists = available_mailing_lists;
            }
            Err(failed_available_lists_request) => {
                bail!(format!("{failed_available_lists_request:#?}"));
            }
        };

        self.clear_target_list();

        lore_session::save_available_lists(fs, &self.mailing_lists, &self.mailing_lists_path)?;

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
        let possible_mailing_lists = self
            .mailing_lists
            .iter()
            .filter(|mailing_list| mailing_list.name().starts_with(&self.target_list))
            .cloned()
            .collect::<Vec<_>>();

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

#[cfg(test)]
mod tests {
    use crate::lore::lore_api_client::MockBlockingLoreAPIClient;

    use super::*;

    #[test]
    fn test_remove_last_target_list_char_empty_target_list() {
        let possible_mailing_lists = vec![MailingList::new("mailing list", "")];
        let highlighted_list_index = 9;
        let mut selection = MailingListSelection {
            mailing_lists: vec![],
            target_list: "".to_string(),
            possible_mailing_lists: possible_mailing_lists.clone(),
            highlighted_list_index,
            mailing_lists_path: "".to_string(),
            lore_api_client: Box::new(MockBlockingLoreAPIClient::new()),
        };

        selection.remove_last_target_list_char();

        // nothing changes
        assert!(selection.target_list.is_empty());
        assert_eq!(selection.possible_mailing_lists, possible_mailing_lists);
        assert_eq!(selection.highlighted_list_index, highlighted_list_index);
    }

    #[test]
    fn test_remove_last_target_list_char_non_empty_target_list() {
        let mailing_lists = vec![
            MailingList::new("target", ""),
            MailingList::new("non target", ""),
        ];
        let highlighted_list_index = 9;
        let mut selection = MailingListSelection {
            mailing_lists: mailing_lists.clone(),
            target_list: "target".to_string(),
            possible_mailing_lists: vec![],
            highlighted_list_index,
            mailing_lists_path: "".to_string(),
            lore_api_client: Box::new(MockBlockingLoreAPIClient::new()),
        };

        selection.remove_last_target_list_char();

        // nothing changes
        assert_eq!(selection.target_list, "targe".to_string());
        assert_eq!(
            selection.possible_mailing_lists,
            vec![mailing_lists[0].clone()]
        );
        assert_eq!(selection.highlighted_list_index, 0);
    }

    #[test]
    fn test_push_char_to_target_list() {
        let mailing_lists = vec![
            MailingList::new("target", ""),
            MailingList::new("non target", ""),
        ];
        let mut selection = MailingListSelection {
            mailing_lists: mailing_lists.clone(),
            target_list: "targe".to_string(),
            possible_mailing_lists: vec![],
            highlighted_list_index: 2,
            mailing_lists_path: "".to_string(),
            lore_api_client: Box::new(MockBlockingLoreAPIClient::new()),
        };

        selection.push_char_to_target_list('t');
        assert_eq!(selection.target_list, "target".to_string());
        assert_eq!(
            selection.possible_mailing_lists,
            vec![mailing_lists[0].clone()]
        );
        assert_eq!(selection.highlighted_list_index, 0);
    }

    #[test]
    fn test_clear_target_list() {
        let mut selection = MailingListSelection {
            mailing_lists: vec![],
            target_list: "some value".to_string(),
            possible_mailing_lists: vec![MailingList::new("match", "")],
            highlighted_list_index: 3,
            mailing_lists_path: "".to_string(),
            lore_api_client: Box::new(MockBlockingLoreAPIClient::new()),
        };

        selection.clear_target_list();
        assert_eq!(selection.target_list, "");
        assert_eq!(selection.highlighted_list_index, 0);
        assert!(selection.possible_mailing_lists.is_empty());
    }

    #[test]
    fn test_process_possible_mailing_lists() {
        struct TestCase {
            mailing_lists: Vec<MailingList>,
            target_list: &'static str,
            highlighted_list_index: usize,
            expected_possible_mailing_lists: Vec<MailingList>,
            test_name: &'static str,
        }

        let test_cases = vec![
            TestCase {
                mailing_lists: vec![],
                target_list: "",
                highlighted_list_index: 0,
                expected_possible_mailing_lists: vec![],
                test_name: "everything empty",
            },
            TestCase {
                mailing_lists: vec![],
                target_list: "",
                highlighted_list_index: 9,
                expected_possible_mailing_lists: vec![],
                test_name: "empty mailing and target list and custom list index",
            },
            TestCase {
                mailing_lists: vec![],
                target_list: "target",
                highlighted_list_index: 0,
                expected_possible_mailing_lists: vec![],
                test_name: "not empty target list, but empty mailing list",
            },
            {
                let mailing_list = MailingList::new("target", "");
                TestCase {
                    mailing_lists: vec![mailing_list.clone()],
                    target_list: "target",
                    highlighted_list_index: 0,
                    expected_possible_mailing_lists: vec![mailing_list],
                    test_name: "mailing list with name = target_list",
                }
            },
            {
                let mailing_list = MailingList::new("Target", "");
                TestCase {
                    mailing_lists: vec![mailing_list.clone()],
                    target_list: "target",
                    highlighted_list_index: 0,
                    expected_possible_mailing_lists: vec![],
                    test_name: "match should be case sensitive",
                }
            },
            {
                let mailing_list_1 = MailingList::new("targetSUFIX", "");
                let mailing_list_2 = MailingList::new("target suffix", "");
                TestCase {
                    mailing_lists: vec![mailing_list_1.clone(), mailing_list_2.clone()],
                    target_list: "target",
                    highlighted_list_index: 2, // this will be changed to 0
                    expected_possible_mailing_lists: vec![mailing_list_1, mailing_list_2],
                    test_name: "two valid mailing lists",
                }
            },
            {
                let invalid_mailing_list_1 = MailingList::new("PREFIXtarget", "");
                let invalid_mailing_list_2 = MailingList::new("PREFIX target", "");
                let invalid_mailing_list_3 = MailingList::new("Target", "");
                TestCase {
                    mailing_lists: vec![
                        invalid_mailing_list_1,
                        invalid_mailing_list_2,
                        invalid_mailing_list_3,
                    ],
                    target_list: "target",
                    highlighted_list_index: 0,
                    expected_possible_mailing_lists: vec![],
                    test_name: "invalida mailing lists",
                }
            },
        ];

        for test_case in test_cases {
            let mut mailing_list_selection = MailingListSelection {
                mailing_lists: test_case.mailing_lists,
                target_list: test_case.target_list.to_string(),
                possible_mailing_lists: vec![],
                highlighted_list_index: test_case.highlighted_list_index,
                mailing_lists_path: "".to_string(),
                lore_api_client: Box::new(MockBlockingLoreAPIClient::new()),
            };

            mailing_list_selection.process_possible_mailing_lists();
            let possible_mailing_lists = mailing_list_selection.possible_mailing_lists;
            assert_eq!(
                possible_mailing_lists, test_case.expected_possible_mailing_lists,
                "Failed possible_mailing_lists assert for test: {}",
                test_case.test_name
            );

            let highlighted_list_index = mailing_list_selection.highlighted_list_index;
            let expected_highlighted_list_index = 0;
            assert_eq!(highlighted_list_index, expected_highlighted_list_index);
        }
    }

    #[test]
    fn test_highlight_below_list() {
        let possible_mailing_lists = vec![
            MailingList::new("some-list", ""),
            MailingList::new("some-list-2", ""),
        ];
        let mut selection = MailingListSelection {
            mailing_lists: vec![],
            target_list: "".to_string(),
            possible_mailing_lists,
            highlighted_list_index: 0,
            mailing_lists_path: "".to_string(),
            lore_api_client: Box::new(MockBlockingLoreAPIClient::new()),
        };

        selection.highlight_below_list();
        assert_eq!(selection.highlighted_list_index, 1);

        // should not increment beyond the end
        selection.highlight_below_list();
        assert_eq!(selection.highlighted_list_index, 1);
    }

    #[test]
    fn test_highlight_above_list() {
        let mailing_list = MailingList::new("some-list", "");
        let mut selection = MailingListSelection {
            mailing_lists: vec![],
            target_list: "".to_string(),
            possible_mailing_lists: vec![mailing_list.clone(), mailing_list.clone()],
            highlighted_list_index: 2,
            mailing_lists_path: "".to_string(),
            lore_api_client: Box::new(MockBlockingLoreAPIClient::new()),
        };

        selection.highlight_above_list();
        assert_eq!(selection.highlighted_list_index, 1);

        selection.highlight_above_list();
        assert_eq!(selection.highlighted_list_index, 0);

        // Should not go negative (saturating_sub)
        selection.highlight_above_list();
        assert_eq!(selection.highlighted_list_index, 0);
    }

    #[test]
    fn test_has_valid_target_list() {
        let mailing_list = MailingList::new("some-list", "");
        let selection_valid = MailingListSelection {
            mailing_lists: vec![],
            target_list: "".to_string(),
            possible_mailing_lists: vec![mailing_list.clone()],
            highlighted_list_index: 0,
            mailing_lists_path: "".to_string(),
            lore_api_client: Box::new(MockBlockingLoreAPIClient::new()),
        };
        assert!(selection_valid.has_valid_target_list());

        let selection_invalid = MailingListSelection {
            mailing_lists: vec![],
            target_list: "".to_string(),
            possible_mailing_lists: vec![mailing_list.clone()],
            highlighted_list_index: 5, // Out of bounds
            mailing_lists_path: "".to_string(),
            lore_api_client: Box::new(MockBlockingLoreAPIClient::new()),
        };
        assert!(!selection_invalid.has_valid_target_list());
    }
}
