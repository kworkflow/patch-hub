use super::*;
use crate::patch::Author;
use std::fs;

struct FakeLoreAPIClient { src_path: String }
impl PatchFeedRequest for FakeLoreAPIClient {
    fn request_patch_feed(self: &Self, target_list: &String, min_index: u32) -> Result<String, FailedFeedRequest> {
        let _ = min_index;
        let _ = target_list;
        Ok(fs::read_to_string(&self.src_path).unwrap())
    }
}

#[test]
fn can_initialize_fresh_lore_session() {
    let lore_session: LoreSession = LoreSession::new("some-list".to_string());

    assert!(lore_session.get_representative_patches_ids().len() == 0,
        "`LoreSession` should initialize with an empty vector of representative patches IDs"
    );
}

#[test]
fn should_process_one_representative_patch() {
    let mut lore_session: LoreSession = LoreSession::new("some-list".to_string());
    let lore_api_client: FakeLoreAPIClient = FakeLoreAPIClient { src_path: "src/lore_session/patch_feed_sample_1.xml".to_string() };
    let message_id: &str = "http://lore.kernel.org/some-subsystem/1234.567-1-john@johnson.com/";
    let patch: &Patch;

    if let Ok(_) = lore_session.process_n_representative_patches(&lore_api_client, 1) {};

    assert_eq!(1, lore_session.get_representative_patches_ids().len(),
        "Should have processed exactly 1 representative patches, but processed {}",
        lore_session.get_representative_patches_ids().len()
    );

    assert_eq!(message_id, lore_session.get_representative_patches_ids().get(0).unwrap(),
        "Wrong representative patch message ID"
    );

    patch = lore_session.get_processed_patch(message_id).unwrap();
    assert_eq!("some/subsystem: Do this and that", patch.get_title(),
        "Wrong title of processed patch"
    );
    assert_eq!(&Author { name: "John Johnson".to_string(), email: "john@johnson.com".to_string() }, patch.get_author(),
        "Wrong author of processed patch"
    );
    assert_eq!(1, patch.get_version(),
        "Wrong version of processed patch"
    );
    assert_eq!(0, patch.get_number_in_series(),
        "Wrong number in series of processed patch"
    );
    assert_eq!(2, patch.get_total_in_series(),
        "Wrong total in series of processed patch"
    );
}

#[test]
fn should_process_multiple_representative_patches() {
    let mut lore_session: LoreSession = LoreSession::new("some-list".to_string());
    let lore_api_client: FakeLoreAPIClient = FakeLoreAPIClient { src_path: "src/lore_session/patch_feed_sample_2.xml".to_string() };
    let message_id_1: &str = "http://lore.kernel.org/some-subsystem/1234.567-1-roberto@silva.br/";
    let message_id_2: &str = "http://lore.kernel.org/some-subsystem/first-patch-lima@luma.rs/";
    let message_id_3: &str = "http://lore.kernel.org/some-subsystem/1234.567-1-john@johnson.com/";

    if let Ok(_) = lore_session.process_n_representative_patches(&lore_api_client, 3) {};

    assert_eq!(3, lore_session.get_representative_patches_ids().len(),
        "Should have processed exactly 3 representative patches, but processed {}",
        lore_session.get_representative_patches_ids().len()
    );

    assert_eq!(message_id_1 , lore_session.get_representative_patches_ids().get(0).unwrap(),
        "Wrong representative patch message ID at index 0"
    );
    assert_eq!(message_id_2 , lore_session.get_representative_patches_ids().get(1).unwrap(),
        "Wrong representative patch message ID at index 1"
    );
    assert_eq!(message_id_3 , lore_session.get_representative_patches_ids().get(2).unwrap(),
        "Wrong representative patch message ID at index 2"
    );
}
