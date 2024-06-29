use lore_peek::lore_session::LoreSession;
use lore_peek::lore_api_client::FailedFeedRequest;
use std::env;

fn main() {
    let args: Vec<_> = env::args().collect();
    let target_list: String;
    let n: u32;
    let mut lore_session: LoreSession;

    if args.len() != 3 {
        panic!("[errno::EINVAL]\n*\tWrong number of arguments\n*\tUsage: cargo run <target_list> <page_size> <page_number>");
    }

    target_list = String::from(&args[1]);
    n = args[2].parse::<u32>().unwrap();

    lore_session = LoreSession::new(target_list);
    if let Err(failed_feed_request) = lore_session.process_n_representative_patches(n) {
        match failed_feed_request {
            FailedFeedRequest::UnknownError(error) => panic!("[FailedFeedRequest::UnknownError]\n*\tFailed to request feed\n*\t{error:#?}"),
            FailedFeedRequest::StatusNotOk(feed_response) => panic!("[FailedFeedRequest::StatusNotOk]\n*\tRequest returned with non-OK status\n*\t{feed_response:#?}"),
            FailedFeedRequest::EndOfFeed => panic!("[FailedFeedRequest::EndOfFeed]\n*\tReached end of feed"),
        }
    };

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
