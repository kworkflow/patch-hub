use lore_peek::lore_session::LoreSession;
use lore_peek::patch::Patch;
use lore_peek::lore_api_client::FailedFeedRequest;
use std::env;

fn main() {
    let args: Vec<_> = env::args().collect();
    let target_list: String;
    let page_size: u32;
    let page_number: u32;
    let mut lore_session: LoreSession;
    let patch_feed_page: Vec<&Patch>;
    let mut index: u32;

    if args.len() != 4 {
        panic!("[errno::EINVAL]\n*\tWrong number of arguments\n*\tUsage: cargo run <target_list> <page_size> <page_number>");
    }

    target_list = String::from(&args[1]);
    page_size = args[2].parse::<u32>().unwrap();
    page_number = args[3].parse::<u32>().unwrap();

    if (page_size == 0) || (page_number == 0) {
        panic!("[errno::EINVAL]\n*\tpage_size and page_number should be non-zero positives")
    }

    lore_session = LoreSession::new(target_list);
    if let Err(failed_feed_request) = lore_session.process_n_representative_patches(page_size * page_number) {
        match failed_feed_request {
            FailedFeedRequest::UnknownError(error) => panic!("[FailedFeedRequest::UnknownError]\n*\tFailed to request feed\n*\t{error:#?}"),
            FailedFeedRequest::StatusNotOk(feed_response) => panic!("[FailedFeedRequest::StatusNotOk]\n*\tRequest returned with non-OK status\n*\t{feed_response:#?}"),
            FailedFeedRequest::EndOfFeed => panic!("[FailedFeedRequest::EndOfFeed]\n*\tReached end of feed"),
        }
    };

    println!("Number of representative patches processed: {}", lore_session.get_representative_patches_ids().len());

    patch_feed_page = lore_session.get_patch_feed_page(page_size, page_number);
    index = page_size * (page_number - 1);
    for patch in patch_feed_page {

        println!(
            "{:03}. V{} | #{:02} | {} | {}",
            index, patch.get_version(), patch.get_total_in_series(), patch.get_title(), patch.get_author().name
        );

        index += 1;
    }
}
