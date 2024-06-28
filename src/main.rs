use lore_peek::lore_session::LoreSession;
use std::env;

fn main() {
    let args: Vec<_> = env::args().collect();
    let target_list: String;
    let n: u32;
    let mut lore_session: LoreSession;

    if args.len() != 3 {
        panic!("Error: Wrong number\nUsage: cargo run <target_list> <n>");
    }

    target_list = String::from(&args[1]);
    n = args[2].parse::<u32>().unwrap();

    lore_session = LoreSession::new(target_list);
    lore_session.process_n_representative_patches(n);

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
