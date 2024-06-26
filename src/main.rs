use lore_peek::lore_session::LoreSession;
use lore_peek::lore_response::LoreResponse;
use serde_xml_rs::from_str;
use std::{env, fs};

fn main() {
    let args: Vec<_> = env::args().collect();
    let amd_gfx_feed: String;
    let path: String;
    let mut lore_session: LoreSession = LoreSession::new();
    let lore_response: LoreResponse;
    let processed_patches_ids: Vec<String>;

    if args.len() != 2 {
        panic!("Need 1 arg");
    }

    path = String::from(&args[1]);

    amd_gfx_feed = fs::read_to_string(&path).unwrap();

    lore_response = from_str(&amd_gfx_feed).unwrap();
    processed_patches_ids = lore_session.process_patches(lore_response);
    lore_session.update_representative_patches(processed_patches_ids);

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
