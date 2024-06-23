use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_xml_rs::from_str;
use std::{collections::HashMap, env, fs};

#[derive(Serialize, Deserialize, Debug)]
struct LoreResponse {
    #[serde(rename = "entry")]
    patches: Vec<Patch>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Patch {
    r#title: String,
    #[serde(default)]
    version: u32,
    #[serde(default)]
    number_in_series: u32,
    #[serde(default)]
    total_in_series: u32,
    author: Author,
    #[serde(rename = "link")]
    message_id: MessageID,
    #[serde(rename = "in-reply-to")]
    in_reply_to: Option<MessageID>,
    updated: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Author {
    name: String,
    email: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct MessageID {
    href: String,
}

fn main() {
    let args: Vec<_> = env::args().collect();
    let amd_gfx_feed: String;
    let path: String;
    let lore_response: LoreResponse;
    let mut representative_patches_vec: Vec<String> = Vec::new();
    let mut patches_map: HashMap<String, Patch> = HashMap::new();
    let mut patches_ids_vec: Vec<String> = Vec::new();

    if args.len() != 2 {
        panic!("Need 1 arg");
    }

    path = String::from(&args[1]);

    amd_gfx_feed = fs::read_to_string(&path).unwrap();

    lore_response = from_str(&amd_gfx_feed).unwrap();

    let re_patch_tag = Regex::new(r"\[[^\]]*[[Rr][Ff][Cc]|[Pp][Aa][Tt][Cc][Hh]][^\[]*\]").unwrap();
    let re_version = Regex::new(r"[v|V] *(\d+)").unwrap();
    let re_series = Regex::new(r"(\d+) */ *(\d+)").unwrap();

    for mut patch in lore_response.patches {
        let patch_tag: &str;
        let title = patch.title.clone();
        match re_patch_tag.find(&title) {
            Some(value) => {
                patch_tag = value.as_str();
                patch.title = patch.title.replace(&patch_tag, "").trim().to_string();
            },
            None => continue,
        }

        match re_version.captures(&patch_tag) {
            Some(value) => match value.get(1) {
                Some(value) => patch.version = value.as_str().parse().unwrap(),
                None => patch.version = 1,
            },
            None => patch.version = 1,
        }

        match re_series.captures(&patch_tag) {
            Some(value) => {
                match value.get(1) {
                    Some(value) => patch.number_in_series = value.as_str().parse().unwrap(),
                    None => patch.number_in_series = 1,
                };
                match value.get(2) {
                    Some(value) => patch.total_in_series = value.as_str().parse().unwrap(),
                    None => patch.total_in_series = 1,
                };
            },
            None => {
                patch.number_in_series = 1;
                patch.total_in_series = 1;
            },
        }

        let message_id_clone = patch.message_id.href.clone();
        patches_ids_vec.push(patch.message_id.href.clone());
        patches_map.insert(message_id_clone, patch);
    }

    for message_id in patches_ids_vec {
        let patch = patches_map.get(&message_id).unwrap();
        if patch.number_in_series == 0 {
            representative_patches_vec.push(patch.message_id.href.clone());
        } else if patch.number_in_series == 1 {
            match &patch.in_reply_to {
                Some(in_reply_to) => match patches_map.get(&in_reply_to.href) {
                    Some(patch_in_reply_to) => {
                        if (patch_in_reply_to.number_in_series == 0)
                            && (patch.version == patch_in_reply_to.version)
                        {
                            continue;
                        }
                    },
                    None => representative_patches_vec.push(patch.message_id.href.clone()),
                },
                None => representative_patches_vec.push(patch.message_id.href.clone()),
            }
        }
    }

    let length = representative_patches_vec.len();
    println!("Number of representative patches processed: {length}");

    for message_id in &representative_patches_vec {
        let patch = patches_map.get(message_id).unwrap();
        println!(
            "V{} | #{:02} | {} | {}",
            patch.version, patch.total_in_series, patch.title, patch.author.name
        );
    }
}
