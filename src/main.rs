use lore_peek::lore_session::LoreSession;
use lore_peek::patch::PatchFeed;
use reqwest::blocking::Response;
use serde_xml_rs::from_str;
use std::env;

const LORE_DOMAIN: &str = r"https://lore.kernel.org/";

fn main() {
    let args: Vec<_> = env::args().collect();
    let mut lore_session: LoreSession = LoreSession::new();
    let patch_feed: PatchFeed;
    let processed_patches_ids: Vec<String>;
    let mut lore_api_request: String = String::from(LORE_DOMAIN);
    let lore_api_response: Response;
    let lore_api_response_body: String;

    if args.len() != 2 {
        panic!("Need 1 arg");
    }

    lore_api_request.push_str(&String::from(&args[1]));
    lore_api_request.push_str(r"/?x=A&q=((s:patch+OR+s:rfc)+AND+NOT+s:re:)&o=0");

    lore_api_response = match reqwest::blocking::get(lore_api_request) {
        Ok(response) => response,
        Err(error) =>  panic!("{error:?}"),
    };

    match lore_api_response.status().as_u16() {
        200 => (),
        _ => panic!("HTTP request didn't return 200 OK"),
    };

    lore_api_response_body = lore_api_response.text().unwrap();
    if lore_api_response_body.eq(r"</feed>") {
        panic!("No more patches")
    };

    patch_feed = from_str(&lore_api_response_body).unwrap();
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
