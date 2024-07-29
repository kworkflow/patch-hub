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

#[test]
fn test_split_patchset_invalid_cases() {
    let ret: Result<Vec<String>, String> = split_patchset("invalid/path");
    assert_eq!(Err(format!("invalid/path: Path doesn't exist")), ret);

    let ret: Result<Vec<String>, String> = split_patchset("src/lore_session/res_split_patchset/not_a_file");
    assert_eq!(Err(format!("src/lore_session/res_split_patchset/not_a_file: Not a file")), ret);
}

#[test]
fn should_split_patchset_without_cover_letter() {
    let ret: Result<Vec<String>, String> = split_patchset(
        "src/lore_session/res_split_patchset/patchset_sample_without_cover_letter.mbx"
    );

    if let Err(_) = ret {
        panic!("Should return a `Vec<String>` type");
    }
    
    let patches = ret.unwrap();

    assert_eq!(
        3, patches.len(),
        "Wrong number of patches"
    );

    assert_eq!(
        fs::read_to_string("src/lore_session/res_split_patchset/expected_patch_1.mbx").unwrap(), patches[0],
        "Wrong patch number 1"
    );

    assert_eq!(
        fs::read_to_string("src/lore_session/res_split_patchset/expected_patch_2.mbx").unwrap(), patches[1],
        "Wrong patch number 2"
    );

    assert_eq!(
        fs::read_to_string("src/lore_session/res_split_patchset/expected_patch_3.mbx").unwrap(), patches[2],
        "Wrong patch number 3"
    );
}

#[test]
fn should_split_patchset_complete() {
    let ret: Result<Vec<String>, String> = split_patchset(
        "src/lore_session/res_split_patchset/patchset_sample_complete.mbx"
    );

    if let Err(_) = ret {
        panic!("Should return a `Vec<String>` type");
    }
    
    let patches = ret.unwrap();

    assert_eq!(
        4, patches.len(),
        "Wrong number of patches"
    );

    assert_eq!(
        fs::read_to_string("src/lore_session/res_split_patchset/expected_cover_letter.cover").unwrap(), patches[0],
        "Wrong cover letter"
    );

    assert_eq!(
        fs::read_to_string("src/lore_session/res_split_patchset/expected_patch_1.mbx").unwrap(), patches[1],
        "Wrong patch number 1"
    );

    assert_eq!(
        fs::read_to_string("src/lore_session/res_split_patchset/expected_patch_2.mbx").unwrap(), patches[2],
        "Wrong patch number 2"
    );

    assert_eq!(
        fs::read_to_string("src/lore_session/res_split_patchset/expected_patch_3.mbx").unwrap(), patches[3],
        "Wrong patch number 3"
    );
}

#[test]
fn should_process_available_lists() {
    let available_lists_response = fs::read_to_string("src/lore_session/res_process_available_lists/available_lists_response-1.html").unwrap();
    let available_lists = process_available_lists(available_lists_response);

    assert_eq!(199, available_lists.len(),
        "Should've processed 199 lists"
    );

    assert_eq!("linux-mm".to_string(), available_lists[0].get_name(),
        "Wrong list name for index 0"
    );
    assert_eq!("Linux-mm Archive on lore.kernel.org".to_string(), available_lists[0].get_description(),
        "Wrong list description for index 0"
    );
    assert_eq!("linux-kselftest".to_string(), available_lists[42].get_name(),
        "Wrong list name for index 42"
    );
    assert_eq!("Linux Kernel Selftest development".to_string(), available_lists[42].get_description(),
        "Wrong list description for index 42"
    );
    assert_eq!("distributions".to_string(), available_lists[99].get_name(),
        "Wrong list name for index 99"
    );
    assert_eq!("Forum for Linux distributions to discuss problems and share PSAs".to_string(), available_lists[99].get_description(),
        "Wrong list description for index 99"
    );
    assert_eq!("grub-devel".to_string(), available_lists[135].get_name(),
        "Wrong list name for index 135"
    );
    assert_eq!("Grub Development Archive on lore.kernel.org".to_string(), available_lists[135].get_description(),
        "Wrong list description for index 135"
    );
    assert_eq!("linux-nilfs".to_string(), available_lists[180].get_name(),
        "Wrong list name for index 180"
    );
    assert_eq!("Linux NILFS development".to_string(), available_lists[180].get_description(),
        "Wrong list description for index 180"
    );
    assert_eq!("linux-sparse".to_string(), available_lists[198].get_name(),
        "Wrong list name for index 198"
    );
    assert_eq!("Linux SPARSE checker discussions".to_string(), available_lists[198].get_description(),
        "Wrong list description for index 198"
    );
}

impl AvailableListsRequest for FakeLoreAPIClient {
    fn request_available_lists(self: &Self, min_index: u32) -> Result<String, FailedAvailableListsRequest> {
        match min_index {
            0 => Ok(fs::read_to_string("src/lore_session/res_process_available_lists/available_lists_response-1.html").unwrap()),
            200 => Ok(fs::read_to_string("src/lore_session/res_process_available_lists/available_lists_response-2.html").unwrap()),
            400 => Ok(fs::read_to_string("src/lore_session/res_process_available_lists/available_lists_response-3.html").unwrap()),
            _ => panic!("Should not try other `min_index` other than `0`, `200`, and `400`"),
        }
    }
}

#[test]
fn should_fetch_all_available_lists() {
    let lore_api_client = FakeLoreAPIClient { src_path: "".to_string() };
    let sorted_available_lists = fetch_available_lists(&lore_api_client).unwrap();

    assert_eq!("accel-config".to_string(), sorted_available_lists[0].get_name(),
        "Wrong list name for index 0"
    );
    assert_eq!("Accel-Config development".to_string(), sorted_available_lists[0].get_description(),
        "Wrong list description for index 0"
    );
    assert_eq!("linux-mediatek".to_string(), sorted_available_lists[159].get_name(),
        "Wrong list name for index 159"
    );
    assert_eq!("Linux-mediatek Archive on lore.kernel.org".to_string(), sorted_available_lists[159].get_description(),
        "Wrong list description for index 159"
    );
    assert_eq!("yocto-toaster".to_string(), sorted_available_lists[319].get_name(),
        "Wrong list name for index 319"
    );
    assert_eq!("Yocto Toaster".to_string(), sorted_available_lists[319].get_description(),
        "Wrong list description for index 319"
    );

    assert_eq!(320, sorted_available_lists.len());
}

#[test]
fn should_generate_patch_reply_template() {
    let patch_sample = fs::read_to_string("src/lore_session/res_generate_patch_reply_template/patch_sample.mbx").unwrap();
    let expected_reply_template = fs::read_to_string("src/lore_session/res_generate_patch_reply_template/expected_reply_template.mbx").unwrap();

    let reply_template = generate_patch_reply_template(&patch_sample);

    assert_eq!(expected_reply_template, reply_template,
        "Reply template wasn't correctly generated"
    )
}
