#![allow(dead_code)]

use regex::Regex;
use std::sync::LazyLock;

use crate::lore::domain::{mailing_list::MailingList, patch::PatchFeed};

/// Parses an Atom feed XML body into a [`PatchFeed`].
pub fn parse_patch_feed(xml: &str) -> Result<PatchFeed, String> {
    serde_xml_rs::from_str(xml).map_err(|e| e.to_string())
}

/// Parses the HTML body returned by the Lore available-lists endpoint into a
/// sorted [`Vec<MailingList>`].
pub fn parse_available_lists(html: &str) -> Vec<MailingList> {
    static RE_PRE_BLOCK: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?s)<pre>(.*?)</pre>"#).unwrap());
    static RE_LIST_NAME: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?s)<a\s*href=".*?">(.*?)</a>"#).unwrap());
    static RE_LIST_DESCRIPTION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?s)</a>\s*(.*?)\s*\*"#).unwrap());

    let mut list_names: Vec<&str> = Vec::new();
    let mut list_descriptions: Vec<&str> = Vec::new();
    let mut available_lists: Vec<MailingList> = Vec::new();

    let pre_blocks: Vec<&str> = RE_PRE_BLOCK
        .captures_iter(html)
        .map(|cap| cap.get(1).unwrap().as_str())
        .collect();

    if pre_blocks.len() < 3 {
        return available_lists;
    }

    for capture in RE_LIST_NAME.captures_iter(pre_blocks[2]) {
        let name = capture.get(1).unwrap().as_str().trim();
        list_names.push(name);
    }

    for capture in RE_LIST_DESCRIPTION.captures_iter(pre_blocks[2]) {
        let description = capture.get(1).unwrap().as_str().trim();
        list_descriptions.push(description);
    }

    for (name, description) in list_names.into_iter().zip(list_descriptions) {
        if name == "all" {
            continue;
        }
        available_lists.push(MailingList::new(name, description));
    }

    available_lists
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn parse_available_lists_processes_html_response() {
        let html = fs::read_to_string(
            "test_samples/lore_session/process_available_lists/available_lists_response-1.html",
        )
        .unwrap();

        let lists = parse_available_lists(&html);

        assert_eq!(199, lists.len(), "Should've processed 199 lists");
        assert_eq!("linux-mm", lists[0].name());
        assert_eq!(
            "Linux-mm Archive on lore.kernel.org",
            lists[0].description()
        );
        assert_eq!("linux-kselftest", lists[42].name());
        assert_eq!("linux-sparse", lists[198].name());
    }

    #[test]
    fn parse_patch_feed_deserializes_xml() {
        let xml = fs::read_to_string(
            "test_samples/lore_session/process_representative_patch/patch_feed_sample_1.xml",
        )
        .unwrap();

        let feed = parse_patch_feed(&xml);
        assert!(feed.is_ok(), "Should parse a valid feed XML");
        assert!(!feed.unwrap().patches().is_empty());
    }

    #[test]
    fn parse_patch_feed_returns_error_for_invalid_xml() {
        let result = parse_patch_feed("this is not xml");
        assert!(result.is_err());
    }
}
