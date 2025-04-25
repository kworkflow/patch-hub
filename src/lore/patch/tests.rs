use super::*;
use serde_xml_rs::from_str;

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
        let title = "[PATCH 3/42] hitchhiker/guide: Life, the Universe and Everything".to_string();
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
        let title = "[RESEND][v7 PATCH 3/42] hitchhiker/guide: Life, the Universe and Everything"
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
