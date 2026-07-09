use std::collections::HashSet;

use serde_xml_rs::from_str;
use tokio::{spawn, sync::mpsc};

use crate::lore::{
    application::{
        dto::{PatchTagSummary, PatchsetDetails},
        errors::LoreError,
        handle::LoreApiHandle,
        messages::LoreApiMessage,
    },
    domain::{mailing_list::MailingList, patch::Patch},
};

pub(crate) fn lore_handle_with_successful_patch_flow() -> LoreApiHandle {
    let (tx, mut rx) = mpsc::channel(8);
    spawn(async move {
        while let Some(message) = rx.recv().await {
            match message {
                LoreApiMessage::FetchFeedPage { reply, .. } => {
                    reply.send(Ok(vec![sample_patch()])).ok();
                }
                LoreApiMessage::FetchPatchsetDetails { reply, .. } => {
                    reply.send(Ok(sample_patchset_details())).ok();
                }
                LoreApiMessage::Shutdown => break,
                other => panic!("unexpected lore message: {}", other.name()),
            }
        }
    });
    LoreApiHandle::new(tx)
}

pub(crate) fn lore_handle_with_patch_details_failure() -> LoreApiHandle {
    let (tx, mut rx) = mpsc::channel(8);
    spawn(async move {
        while let Some(message) = rx.recv().await {
            match message {
                LoreApiMessage::FetchPatchsetDetails { reply, .. } => {
                    reply
                        .send(Err(LoreError::ActorUnavailable(
                            "lore actor unavailable in test".to_string(),
                        )))
                        .ok();
                }
                LoreApiMessage::Shutdown => break,
                other => panic!("unexpected lore message: {}", other.name()),
            }
        }
    });
    LoreApiHandle::new(tx)
}

pub(crate) fn lore_handle_with_persistence() -> LoreApiHandle {
    let (tx, mut rx) = mpsc::channel(8);
    spawn(async move {
        while let Some(message) = rx.recv().await {
            match message {
                LoreApiMessage::SaveBookmarks { reply, .. } => {
                    reply.send(Ok(())).ok();
                }
                LoreApiMessage::SaveReviewed { reply, .. } => {
                    reply.send(Ok(())).ok();
                }
                LoreApiMessage::Shutdown => break,
                other => panic!("unexpected lore message: {}", other.name()),
            }
        }
    });
    LoreApiHandle::new(tx)
}

pub(crate) fn sample_mailing_list() -> MailingList {
    MailingList::new("test-list", "Test list")
}

pub(crate) fn sample_patch() -> Patch {
    from_str(
        r#"
        <entry xmlns:thr="http://purl.org/syndication/thread/1.0">
            <author>
                <name>Foo Bar</name>
                <email>foo@bar.example</email>
            </author>
            <title>[PATCH 1/1] test patch</title>
            <updated>2024-07-06T19:15:48Z</updated>
            <link href="http://lore.kernel.org/test-list/1234-1-foo@bar.example" />
            <id>urn:uuid:123-abcd-1f2a3b</id>
            <content></content>
        </entry>
        "#,
    )
    .expect("sample patch XML should deserialize")
}

pub(crate) fn sample_patchset_details() -> PatchsetDetails {
    PatchsetDetails {
        patchset_path: "/tmp/patchset.mbx".to_string(),
        raw_patches: vec![sample_raw_patch()],
        tag_summary: vec![empty_tag_summary()],
    }
}

pub(crate) fn sample_raw_patch() -> String {
    "Subject: [PATCH] test\n\nBody\n---\n file.txt | 1 +\n 1 file changed, 1 insertion(+)\n"
        .to_string()
}

pub(crate) fn empty_tag_summary() -> PatchTagSummary {
    PatchTagSummary {
        reviewed_by: HashSet::new(),
        tested_by: HashSet::new(),
        acked_by: HashSet::new(),
    }
}
