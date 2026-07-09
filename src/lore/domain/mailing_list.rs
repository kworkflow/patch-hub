use std::cmp::Ordering;

use derive_getters::Getters;
use serde::{Deserialize, Serialize};

#[derive(Getters, Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct MailingList {
    name: String,
    description: String,
}

impl MailingList {
    pub fn new(name: &str, description: &str) -> Self {
        MailingList {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}

impl Ord for MailingList {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for MailingList {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_deserialize_mailing_list() {
        let expected_mailing_list = MailingList::new("list-name", "List Description");
        let serialized_mailing_list = r#"{"name":"list-name","description":"List Description"}"#;
        let deserialized_mailing_list: MailingList =
            serde_json::from_str(serialized_mailing_list).unwrap();

        assert_eq!(
            expected_mailing_list, deserialized_mailing_list,
            "Wrong deserialization of mailing list"
        )
    }

    #[test]
    fn can_serialize_mailing_list() {
        let expected_serialized_mailing_list =
            r#"{"name":"list-name","description":"List Description"}"#;
        let mailing_list = MailingList::new("list-name", "List Description");
        let serialized_mailing_list = serde_json::to_string(&mailing_list).unwrap();

        assert_eq!(
            expected_serialized_mailing_list, serialized_mailing_list,
            "Wrong serialization of mailing list"
        )
    }

    #[test]
    fn should_sort_mailing_list_vec() {
        let mut mailing_list_vec = vec![
            MailingList::new("deref", "description"),
            MailingList::new("unit", "description"),
            MailingList::new("closure", "description"),
            MailingList::new("owner", "description"),
            MailingList::new("borrow", "description"),
        ];
        let mailing_list_vec_for_cmp = mailing_list_vec.clone();
        mailing_list_vec.sort();

        assert_eq!(
            mailing_list_vec_for_cmp[4], mailing_list_vec[0],
            "Wrong mailing list at index 0"
        );
        assert_eq!(
            mailing_list_vec_for_cmp[2], mailing_list_vec[1],
            "Wrong mailing list at index 1"
        );
        assert_eq!(
            mailing_list_vec_for_cmp[0], mailing_list_vec[2],
            "Wrong mailing list at index 2"
        );
        assert_eq!(
            mailing_list_vec_for_cmp[3], mailing_list_vec[3],
            "Wrong mailing list at index 3"
        );
        assert_eq!(
            mailing_list_vec_for_cmp[1], mailing_list_vec[4],
            "Wrong mailing list at index 4"
        );
    }
}
