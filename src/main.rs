use lore_peek::lore_response::LoreResponse;
use lore_peek::patch::{Patch, PatchRegex};
use serde_xml_rs::from_str;
use std::{collections::HashMap, env, fs};

fn main() {
    let args: Vec<_> = env::args().collect();
    let amd_gfx_feed: String;
    let path: String;
    let lore_response: LoreResponse;
    let patch_regex: PatchRegex = PatchRegex::new();
    let mut representative_patches_vec: Vec<String> = Vec::new();
    let mut patches_map: HashMap<String, Patch> = HashMap::new();
    let mut patches_ids_vec: Vec<String> = Vec::new();

    if args.len() != 2 {
        panic!("Need 1 arg");
    }

    path = String::from(&args[1]);

    amd_gfx_feed = fs::read_to_string(&path).unwrap();

    lore_response = from_str(&amd_gfx_feed).unwrap();

    for mut patch in lore_response.get_patches() {
        patch.update_patch_metadata(&patch_regex);
        patches_ids_vec.push(patch.get_message_id().href.clone());
        patches_map.insert(patch.get_message_id().href.clone(), patch);
    }

    for message_id in patches_ids_vec {
        let patch = patches_map.get(&message_id).unwrap();
        if patch.get_number_in_series() == 0 {
            representative_patches_vec.push(patch.get_message_id().href.clone());
        } else if patch.get_number_in_series() == 1 {
            match &patch.get_in_reply_to() {
                Some(in_reply_to) => match patches_map.get(&in_reply_to.href) {
                    Some(patch_in_reply_to) => {
                        if (patch_in_reply_to.get_number_in_series() == 0)
                            && (patch.get_version() == patch_in_reply_to.get_version())
                        {
                            continue;
                        }
                    },
                    None => representative_patches_vec.push(patch.get_message_id().href.clone()),
                },
                None => representative_patches_vec.push(patch.get_message_id().href.clone()),
            }
        }
    }

    let length = representative_patches_vec.len();
    println!("Number of representative patches processed: {length}");

    for message_id in &representative_patches_vec {
        let patch = patches_map.get(message_id).unwrap();
        println!(
            "V{} | #{:02} | {} | {}",
            patch.get_version(), patch.get_total_in_series(), patch.get_title(), patch.get_author().name
        );
    }
}
