use io::Read;

use super::*;
use crate::patch::Author;
use std::fs;

struct FakeLoreAPIClient { src_path: String }
impl PatchFeedRequest for FakeLoreAPIClient {
    fn request_patch_feed(&self, target_list: &str, min_index: u32) -> Result<String, FailedFeedRequest> {
        let _ = min_index;
        let _ = target_list;
        Ok(fs::read_to_string(&self.src_path).unwrap())
    }
}

#[test]
fn can_initialize_fresh_lore_session() {
    let lore_session: LoreSession = LoreSession::new("some-list".to_string());

    assert!(lore_session.get_representative_patches_ids().is_empty(),
        "`LoreSession` should initialize with an empty vector of representative patches IDs"
    );
}

#[test]
fn should_process_one_representative_patch() {
    let mut lore_session: LoreSession = LoreSession::new("some-list".to_string());
    let lore_api_client: FakeLoreAPIClient = FakeLoreAPIClient { src_path: "src/test_samples/lore_session/process_representative_patch/patch_feed_sample_1.xml".to_string() };
    let message_id: &str = "http://lore.kernel.org/some-subsystem/1234.567-1-john@johnson.com/";
    

    if lore_session.process_n_representative_patches(&lore_api_client, 1).is_ok() {};

    assert_eq!(1, lore_session.get_representative_patches_ids().len(),
        "Should have processed exactly 1 representative patches, but processed {}",
        lore_session.get_representative_patches_ids().len()
    );

    assert_eq!(message_id, lore_session.get_representative_patches_ids().first().unwrap(),
        "Wrong representative patch message ID"
    );

    let patch: &Patch = lore_session.get_processed_patch(message_id).unwrap();
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
    let lore_api_client: FakeLoreAPIClient = FakeLoreAPIClient { src_path: "src/test_samples/lore_session/process_representative_patch/patch_feed_sample_2.xml".to_string() };
    let message_id_1: &str = "http://lore.kernel.org/some-subsystem/1234.567-1-roberto@silva.br/";
    let message_id_2: &str = "http://lore.kernel.org/some-subsystem/first-patch-lima@luma.rs/";
    let message_id_3: &str = "http://lore.kernel.org/some-subsystem/1234.567-1-john@johnson.com/";

    if lore_session.process_n_representative_patches(&lore_api_client, 3).is_ok() {};

    assert_eq!(3, lore_session.get_representative_patches_ids().len(),
        "Should have processed exactly 3 representative patches, but processed {}",
        lore_session.get_representative_patches_ids().len()
    );

    assert_eq!(message_id_1 , lore_session.get_representative_patches_ids().first().unwrap(),
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
    assert_eq!(Err("invalid/path: Path doesn't exist".to_string()), ret);

    let ret: Result<Vec<String>, String> = split_patchset("src/test_samples/lore_session/split_patchset/not_a_file");
    assert_eq!(Err("src/test_samples/lore_session/split_patchset/not_a_file: Not a file".to_string()), ret);
}

#[test]
fn should_split_patchset_without_cover_letter() {
    let ret: Result<Vec<String>, String> = split_patchset(
        "src/test_samples/lore_session/split_patchset/patchset_sample_without_cover_letter.mbx"
    );

    if ret.is_err() {
        panic!("Should return a `Vec<String>` type");
    }
    
    let patches = ret.unwrap();

    assert_eq!(
        3, patches.len(),
        "Wrong number of patches"
    );

    assert_eq!(
        fs::read_to_string("src/test_samples/lore_session/split_patchset/expected_patch_1.mbx").unwrap(), patches[0],
        "Wrong patch number 1"
    );

    assert_eq!(
        fs::read_to_string("src/test_samples/lore_session/split_patchset/expected_patch_2.mbx").unwrap(), patches[1],
        "Wrong patch number 2"
    );

    assert_eq!(
        fs::read_to_string("src/test_samples/lore_session/split_patchset/expected_patch_3.mbx").unwrap(), patches[2],
        "Wrong patch number 3"
    );
}

#[test]
fn should_split_patchset_complete() {
    let ret: Result<Vec<String>, String> = split_patchset(
        "src/test_samples/lore_session/split_patchset/patchset_sample_complete.mbx"
    );

    if ret.is_err() {
        panic!("Should return a `Vec<String>` type");
    }
    
    let patches = ret.unwrap();

    assert_eq!(
        4, patches.len(),
        "Wrong number of patches"
    );

    assert_eq!(
        fs::read_to_string("src/test_samples/lore_session/split_patchset/expected_cover_letter.cover").unwrap(), patches[0],
        "Wrong cover letter"
    );

    assert_eq!(
        fs::read_to_string("src/test_samples/lore_session/split_patchset/expected_patch_1.mbx").unwrap(), patches[1],
        "Wrong patch number 1"
    );

    assert_eq!(
        fs::read_to_string("src/test_samples/lore_session/split_patchset/expected_patch_2.mbx").unwrap(), patches[2],
        "Wrong patch number 2"
    );

    assert_eq!(
        fs::read_to_string("src/test_samples/lore_session/split_patchset/expected_patch_3.mbx").unwrap(), patches[3],
        "Wrong patch number 3"
    );
}

#[test]
fn should_process_available_lists() {
    let available_lists_response = fs::read_to_string("src/test_samples/lore_session/process_available_lists/available_lists_response-1.html").unwrap();
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
    fn request_available_lists(&self, min_index: u32) -> Result<String, FailedAvailableListsRequest> {
        match min_index {
            0 => Ok(fs::read_to_string("src/test_samples/lore_session/process_available_lists/available_lists_response-1.html").unwrap()),
            200 => Ok(fs::read_to_string("src/test_samples/lore_session/process_available_lists/available_lists_response-2.html").unwrap()),
            400 => Ok(fs::read_to_string("src/test_samples/lore_session/process_available_lists/available_lists_response-3.html").unwrap()),
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
    let patch_sample = fs::read_to_string("src/test_samples/lore_session/generate_patch_reply_template/patch_sample.mbx").unwrap();
    let expected_reply_template = fs::read_to_string("src/test_samples/lore_session/generate_patch_reply_template/expected_reply_template.mbx").unwrap();

    let reply_template = generate_patch_reply_template(&patch_sample);

    assert_eq!(expected_reply_template, reply_template,
        "Reply template wasn't correctly generated"
    )
}

fn commands_eq(cmd1: &Command, cmd2: &Command) -> bool {
    cmd1.get_program() == cmd2.get_program() &&
    cmd1.get_args().collect::<Vec<_>>() == cmd2.get_args().collect::<Vec<_>>()
}

#[test]
fn should_extract_git_reply_command_from_patch_html()
{
    let patch_html = fs::read_to_string("src/test_samples/lore_session/extract_git_reply_command/patch_lore_sample.html").unwrap();
    let mut expected_git_reply_command = Command::new("git");
    expected_git_reply_command
        .arg("send-email")
        .arg("--dry-run") // Remove this after validating
        .arg("--suppress-cc=all")
        .arg("--in-reply-to=1234.567-3-john@johnson.com")
        .arg("--to=foo@bar.com")
        .arg("--cc=bar@foo.com")
        .arg("--cc=foo@list.org")
        .arg("--cc=bar@list.org");

    let git_reply_command = extract_git_reply_command(&patch_html);

    assert!(commands_eq(&expected_git_reply_command, &git_reply_command),
        "Wrong git reply command\nExpected:{:?}\n  Actual:{:?}", expected_git_reply_command, git_reply_command
    );
}

fn files_eq(path1: &str, path2: &str) -> io::Result<bool> {
    let mut file1 = File::open(path1)?;
    let mut file2 = File::open(path2)?;

    let mut buf1 = Vec::new();
    let mut buf2 = Vec::new();

    file1.read_to_end(&mut buf1)?;
    file2.read_to_end(&mut buf2)?;

    Ok(buf1 == buf2)
}

impl PatchHTMLRequest for FakeLoreAPIClient {
    fn request_patch_html(&self, _target_list: &str, message_id: &str) -> Result<String, FailedPatchHTMLRequest> {
        let patch_html = "git-send-email(1): ".to_owned();
        let patch_html = match message_id {
            "1234.567-0-foo@bar.foo.bar" => patch_html + "git send-email --in-reply-to=1234.567-0-foo@bar.foo.bar --to=foo@bar.foo.bar /path/to/YOUR_REPLY",
            "1234.567-1-foo@bar.foo.bar" => patch_html + "git send-email --in-reply-to=1234.567-1-foo@bar.foo.bar --to=foo@bar.foo.bar /path/to/YOUR_REPLY",
            "1234.567-2-foo@bar.foo.bar" => patch_html + "git send-email --in-reply-to=1234.567-2-foo@bar.foo.bar --to=foo@bar.foo.bar /path/to/YOUR_REPLY",
            "1234.567-3-foo@bar.foo.bar" => patch_html + "git send-email --in-reply-to=1234.567-3-foo@bar.foo.bar --to=foo@bar.foo.bar /path/to/YOUR_REPLY",
            _ => panic!("Should not try other message-IDs than `1234.567-{{0,1,2,3}}`"),
        };

        Ok(patch_html.to_owned())
    }
}

#[test]
fn should_prepare_reply_patchset_with_reviewed_by() {
    let tmp_dir = Command::new("mktemp")
        .arg("--directory")
        .output()
        .unwrap();
    let tmp_dir = Path::new(
        std::str::from_utf8(&tmp_dir.stdout).unwrap().trim()
    );

    let mut expected_git_reply_command_0 = Command::new("git");
    expected_git_reply_command_0
        .arg("send-email")
        .arg("--dry-run") // Remove this after validating
        .arg("--suppress-cc=all")
        .arg("--in-reply-to=1234.567-0-foo@bar.foo.bar")
        .arg("--to=foo@bar.foo.bar")
        .arg(format!("{}/1234.567-0-foo@bar.foo.bar-reply.mbx", tmp_dir.display()));
    let mut expected_git_reply_command_1 = Command::new("git");
    expected_git_reply_command_1
        .arg("send-email")
        .arg("--dry-run") // Remove this after validating
        .arg("--suppress-cc=all")
        .arg("--in-reply-to=1234.567-1-foo@bar.foo.bar")
        .arg("--to=foo@bar.foo.bar")
        .arg(format!("{}/1234.567-1-foo@bar.foo.bar-reply.mbx", tmp_dir.display()));
    let mut expected_git_reply_command_2 = Command::new("git");
    expected_git_reply_command_2
        .arg("send-email")
        .arg("--dry-run") // Remove this after validating
        .arg("--suppress-cc=all")
        .arg("--in-reply-to=1234.567-2-foo@bar.foo.bar")
        .arg("--to=foo@bar.foo.bar")
        .arg(format!("{}/1234.567-2-foo@bar.foo.bar-reply.mbx", tmp_dir.display()));
    let mut expected_git_reply_command_3 = Command::new("git");
    expected_git_reply_command_3
        .arg("send-email")
        .arg("--dry-run") // Remove this after validating
        .arg("--suppress-cc=all")
        .arg("--in-reply-to=1234.567-3-foo@bar.foo.bar")
        .arg("--to=foo@bar.foo.bar")
        .arg(format!("{}/1234.567-3-foo@bar.foo.bar-reply.mbx", tmp_dir.display()));

    let expected_git_reply_commands = vec![
        expected_git_reply_command_0,
        expected_git_reply_command_1,
        expected_git_reply_command_2,
        expected_git_reply_command_3
    ];

    let lore_api_client = FakeLoreAPIClient{ src_path: "".to_owned() };

    let patches = vec![
        fs::read_to_string("src/test_samples/lore_session/prepare_reply_w_reviewed_by/cover_letter.cover").unwrap(),
        fs::read_to_string("src/test_samples/lore_session/prepare_reply_w_reviewed_by/patch_1.mbx").unwrap(),
        fs::read_to_string("src/test_samples/lore_session/prepare_reply_w_reviewed_by/patch_2.mbx").unwrap(),
        fs::read_to_string("src/test_samples/lore_session/prepare_reply_w_reviewed_by/patch_3.mbx").unwrap(),
    ];

    let git_reply_commands = prepare_reply_patchset_with_reviewed_by(
        &lore_api_client, tmp_dir, "all", &patches, "Bar Foo <bar@foo.bar.foo>"
    ).unwrap();

    for (expected, actual) in expected_git_reply_commands.iter().zip(git_reply_commands.iter()) {
        assert!(commands_eq(expected, actual),
            "Wrong git reply command\nExpected:{:?}\n  Actual:{:?}", expected, actual 
        );

    }

    for i in 0..=3 {
        let expected_path = format!("src/test_samples/lore_session/prepare_reply_w_reviewed_by/expected_patch_{}-reply.mbx", i);
        let actual_path = format!("{}/1234.567-{}-foo@bar.foo.bar-reply.mbx", tmp_dir.display(), i);
        assert!(files_eq(&expected_path, &actual_path).unwrap(),
            "Wrong reply with reviewed-by generated\nExpected ({}):\n{}\n  Actual({}):\n{}\n",
            &expected_path, &fs::read_to_string(&expected_path).unwrap(),
            &actual_path, &fs::read_to_string(&actual_path).unwrap()
        );
    }

    fs::remove_dir_all(tmp_dir).unwrap();
}

#[test]
fn should_get_local_git_signature() {
    let mocked_git_repo = Command::new("mktemp")
        .arg("--directory")
        .output()
        .unwrap();
    let mocked_git_repo = Path::new(
        std::str::from_utf8(&mocked_git_repo.stdout).unwrap().trim()
    );

    let _ =Command::new("git")
        .arg("-C")
        .arg(format!("{}", mocked_git_repo.display()))
        .arg("init")
        .output()
        .unwrap();

    let _ =Command::new("git")
        .arg("-C")
        .arg(format!("{}", mocked_git_repo.display()))
        .arg("config")
        .arg("--local")
        .arg("user.name")
        .arg("Foo Bar")
        .output()
        .unwrap();

    let _ =Command::new("git")
        .arg("-C")
        .arg(format!("{}", mocked_git_repo.display()))
        .arg("config")
        .arg("--local")
        .arg("user.email")
        .arg("foo@bar.foo.bar")
        .output()
        .unwrap();

    let (git_user_name, git_user_email) = get_git_signature(mocked_git_repo.to_str().unwrap());

    assert_eq!("Foo Bar".to_owned(), git_user_name,
        "Wrong `git config user.name` value"
    );

    assert_eq!("foo@bar.foo.bar".to_owned(), git_user_email,
        "Wrong `git config user.email` value"
    );

    fs::remove_dir_all(mocked_git_repo).unwrap();
}
