use lore_peek::lore_session::LoreSession;
use lore_peek::patch::PatchFeed;
use serde_xml_rs::from_str;
use std::{env, fs};

fn main() {
    let args: Vec<_> = env::args().collect();
    let mut lore_session: LoreSession = LoreSession::new();
    let patch_feed: PatchFeed;
    let processed_patches_ids: Vec<String>;

    if args.len() != 2 {
        panic!("Need 1 arg");
    }

    patch_feed = from_str(&fs::read_to_string(&String::from(&args[1])).unwrap()).unwrap();
    processed_patches_ids = lore_session.process_patches(patch_feed);
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
