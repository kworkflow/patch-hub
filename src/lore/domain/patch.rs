pub use super::ids::MessageID;

use derive_getters::Getters;
use regex::Regex;
use serde::{Deserialize, Serialize};

use std::fmt::{self, Display, Formatter};

#[derive(Getters, Serialize, Deserialize, Debug, Clone)]
pub struct PatchFeed {
    #[serde(rename = "entry")]
    patches: Vec<Patch>,
}

#[derive(Getters, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Patch {
    r#title: String,
    #[serde(default = "default_version")]
    #[getter(skip)]
    version: usize,
    #[serde(default = "default_number_in_series")]
    #[getter(skip)]
    number_in_series: usize,
    #[serde(default = "default_total_in_series")]
    #[getter(skip)]
    total_in_series: usize,
    author: Author,
    #[serde(rename = "link")]
    message_id: MessageID,
    #[serde(rename = "thr:in-reply-to")]
    in_reply_to: Option<MessageID>,
    updated: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Eq, PartialEq)]
pub struct Author {
    pub name: String,
    pub email: String,
}

impl Display for Author {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} <{}>", self.name, self.email)?;
        Ok(())
    }
}

fn default_version() -> usize {
    1
}
fn default_number_in_series() -> usize {
    1
}
fn default_total_in_series() -> usize {
    1
}

impl Patch {
    pub fn version(&self) -> usize {
        self.version
    }

    pub fn number_in_series(&self) -> usize {
        self.number_in_series
    }

    pub fn total_in_series(&self) -> usize {
        self.total_in_series
    }

    pub fn update_patch_metadata(&mut self, patch_regex: &PatchRegex) {
        let patch_tag: String = match self.get_patch_tag(&patch_regex.re_patch_tag) {
            Some(value) => value.to_string(),
            None => return,
        };

        self.remove_patch_tag_from_title(&patch_tag);
        self.set_version(&patch_tag, &patch_regex.re_patch_version);
        self.set_number_in_series(&patch_tag, &patch_regex.re_patch_series);
        self.set_total_in_series(&patch_tag, &patch_regex.re_patch_series);
    }

    fn get_patch_tag(&self, re_patch_tag: &Regex) -> Option<&str> {
        match re_patch_tag.find(&self.title) {
            Some(patch_tag) => Some(patch_tag.as_str()),
            None => None,
        }
    }

    fn remove_patch_tag_from_title(&mut self, patch_tag: &str) {
        self.title = self.title.replace(patch_tag, "").trim().to_string();
    }

    fn set_version(&mut self, patch_tag: &str, re_patch_version: &Regex) {
        if let Some(capture) = re_patch_version.captures(patch_tag) {
            if let Some(version) = capture.get(1) {
                self.version = version.as_str().parse().unwrap();
            }
        }
    }

    fn set_number_in_series(&mut self, patch_tag: &str, re_patch_series: &Regex) {
        if let Some(capture) = re_patch_series.captures(patch_tag) {
            if let Some(number_in_series) = capture.get(1) {
                self.number_in_series = number_in_series.as_str().parse().unwrap();
            }
        }
    }

    fn set_total_in_series(&mut self, patch_tag: &str, re_patch_series: &Regex) {
        if let Some(capture) = re_patch_series.captures(patch_tag) {
            if let Some(total_in_series) = capture.get(2) {
                self.total_in_series = total_in_series.as_str().parse().unwrap();
            }
        }
    }
}

pub struct PatchRegex {
    pub re_patch_tag: Regex,
    pub re_patch_version: Regex,
    pub re_patch_series: Regex,
}

impl Default for PatchRegex {
    fn default() -> Self {
        Self::new()
    }
}

impl PatchRegex {
    pub fn new() -> PatchRegex {
        let re_patch_tag = Regex::new(r"(?i)\[[^\]]*(PATCH|RFC)[^\[]*\]").unwrap();
        let re_patch_version = Regex::new(r"[v|V] *(\d+)").unwrap();
        let re_patch_series = Regex::new(r"(\d+) */ *(\d+)").unwrap();

        PatchRegex {
            re_patch_tag,
            re_patch_version,
            re_patch_series,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_xml_rs::from_str;

    use super::*;

    #[test]
    fn can_deserialize_patch_without_in_reply_to() {
        let expected_patch: Patch = {
            let title = "[PATCH 0/42] hitchhiker/guide: Complete Collection".to_string();
            let author = Author {
                name: "Foo Bar".to_string(),
                email: "foo@bar.foo.bar".to_string(),
            };
            let message_id = MessageID {
                href: "http://lore.kernel.org/some-list/1234-1-foo@bar.foo.bar".to_string(),
            };
            let updated = "2024-07-06T19:15:48Z".to_string();
            Patch {
                title,
                author,
                version: 1,
                number_in_series: 1,
                total_in_series: 1,
                message_id,
                in_reply_to: None,
                updated,
            }
        };
        let serialized_patch: &str = r#"
            <entry xmlns:thr="http://purl.org/syndication/thread/1.0">
                <author>
                    <name>Foo Bar</name>
                    <email>foo@bar.foo.bar</email>
                </author>
                <title>[PATCH 0/42] hitchhiker/guide: Complete Collection</title>
                <updated>2024-07-06T19:15:48Z</updated>
                <link
                    href="http://lore.kernel.org/some-list/1234-1-foo@bar.foo.bar" />
                <id>urn:uuid:123-abcd-1f2a3b</id>
                <content></content>
            </entry>
        "#;

        let actual_patch: Patch = from_str(serialized_patch).unwrap();

        assert_eq!(
            expected_patch, actual_patch,
            "An entry from a patch feed should deserialize into"
        )
    }

    #[test]
    fn can_deserialize_patch_with_in_reply_to() {
        let expected_patch: Patch = {
            let title =
                "[PATCH 3/42] hitchhiker/guide: Life, the Universe and Everything".to_string();
            let author = Author {
                name: "Foo Bar".to_string(),
                email: "foo@bar.foo.bar".to_string(),
            };
            let message_id = MessageID {
                href: "http://lore.kernel.org/some-list/1234-2-foo@bar.foo.bar".to_string(),
            };
            let in_reply_to = Some(MessageID {
                href: "http://lore.kernel.org/some-list/1234-1-foo@bar.foo.bar".to_string(),
            });
            let updated = "2024-07-06T19:16:53Z".to_string();
            Patch {
                title,
                author,
                version: 1,
                number_in_series: 1,
                total_in_series: 1,
                message_id,
                in_reply_to,
                updated,
            }
        };
        let serialized_patch: &str = r#"
            <entry xmlns:thr="http://purl.org/syndication/thread/1.0">
                <author>
                    <name>Foo Bar</name>
                    <email>foo@bar.foo.bar</email>
                </author>
                <title>[PATCH 3/42] hitchhiker/guide: Life, the Universe and Everything</title>
                <updated>2024-07-06T19:16:53Z</updated>
                <link
                    href="http://lore.kernel.org/some-list/1234-2-foo@bar.foo.bar" />
                <id>urn:uuid:123-abcd-1f2a3b</id>
                <thr:in-reply-to
                    ref="urn:uuid:123-abcd-1f2a3b"
                    href="http://lore.kernel.org/some-list/1234-1-foo@bar.foo.bar" />
                <content></content>
            </entry>
        "#;

        let actual_patch: Patch = from_str(serialized_patch).unwrap();

        assert_eq!(
            expected_patch, actual_patch,
            "An entry from a patch feed should deserialize into"
        )
    }

    #[test]
    fn test_update_patch_metadata() {
        let patch_regex: PatchRegex = PatchRegex::new();
        let mut patch: Patch = {
            let title =
                "[RESEND][v7 PATCH 3/42] hitchhiker/guide: Life, the Universe and Everything"
                    .to_string();
            let author = Author {
                name: "Foo Bar".to_string(),
                email: "foo@bar.foo.bar".to_string(),
            };
            let message_id = MessageID {
                href: "http://lore.kernel.org/some-list/1234-2-foo@bar.foo.bar".to_string(),
            };
            let in_reply_to = Some(MessageID {
                href: "http://lore.kernel.org/some-list/1234-1-foo@bar.foo.bar".to_string(),
            });
            let updated = "2024-07-06T19:16:53Z".to_string();
            Patch {
                title,
                author,
                version: 1,
                number_in_series: 1,
                total_in_series: 1,
                message_id,
                in_reply_to,
                updated,
            }
        };

        patch.update_patch_metadata(&patch_regex);

        assert_eq!(
            "[RESEND] hitchhiker/guide: Life, the Universe and Everything",
            patch.title(),
            "The title should have the patch tag `[v7 PATCH 3/42]` stripped"
        );
        assert_eq!(7, patch.version(), "Wrong version!");
        assert_eq!(3, patch.number_in_series(), "Wrong number in series!");
        assert_eq!(42, patch.total_in_series(), "Wrong total in series!");
    }
}
