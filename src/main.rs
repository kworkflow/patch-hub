use lore_peek::lore_session::LoreSession;
use lore_peek::lore_api_client::{FailedFeedResquest, LoreAPIClient};
use lore_peek::patch::PatchFeed;
use serde_xml_rs::from_str;
use std::env;

const LORE_PAGE_SIZE: u32 = 200;

fn main() {
    let args: Vec<_> = env::args().collect();
    let mut lore_session: LoreSession = LoreSession::new();
    let mut patch_feed: PatchFeed;
    let mut processed_patches_ids: Vec<String>;
    let target_list: String;
    let n_patchsets: u32;
    let mut min_index: u32 = 0;

    if args.len() != 3 {
        panic!("Error: Wrong number\nUsage: cargo run <target_list> <n_patchsets>");
    }

    target_list = String::from(&args[1]);
    n_patchsets = args[2].parse::<u32>().unwrap();

    while lore_session.get_representative_patches_ids().len() < usize::try_from(n_patchsets).unwrap() {
        match LoreAPIClient::request_patch_feed(&target_list, min_index) {
            Ok(feed_response_body) => patch_feed = from_str(&feed_response_body).unwrap(),
            Err(failed_feed_request) => match failed_feed_request {
                FailedFeedResquest::UnknowError(error) => panic!("{error:#?}"),
                FailedFeedResquest::StatusNotOk(status_code) => panic!("Lore request returned status code {status_code}"),
                FailedFeedResquest::EndOfFeed => break,
            },
        }

        processed_patches_ids = lore_session.process_patches(patch_feed);
        lore_session.update_representative_patches(processed_patches_ids);

        min_index = min_index + LORE_PAGE_SIZE;
    }

    let representative_patches_ids: &Vec<String> = lore_session.get_representative_patches_ids();
    println!("Number of representative patches processed: {}", representative_patches_ids.len());

    for message_id in representative_patches_ids {
        let patch = lore_session.get_processed_patch(message_id).unwrap();
        println!(
            "V{} | #{:02} | {} | {}",
            patch.get_version(), patch.get_total_in_series(), patch.get_title(), patch.get_author().name
        );
    }
}
