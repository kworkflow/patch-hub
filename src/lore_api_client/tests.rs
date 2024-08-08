use super::*;
use crate::patch::PatchFeed;

#[test]
#[ignore = "network-io"]
fn blocking_client_can_request_valid_patch_feed() {
    let lore_api_client = BlockingLoreAPIClient::new();

    let patch_feed = lore_api_client.request_patch_feed("amd-gfx", 0).unwrap();
    let patch_feed: PatchFeed = serde_xml_rs::from_str(&patch_feed).unwrap();
    let patches = patch_feed.get_patches();

    assert_eq!(200, patches.len(),
        "Should successfully request patch feed with 200 patches"
    );
}

#[test]
#[ignore = "network-io"]
fn blocking_client_should_detect_failed_patch_feed_request() {
    let lore_api_client = BlockingLoreAPIClient::new();

    if let Err(failed_feed_request) = lore_api_client.request_patch_feed("invalid-list", 0) {
        match failed_feed_request {
            FailedFeedRequest::StatusNotOk(_) => (),
            _ => panic!("Invalid request should return non 200 OK status.\n{failed_feed_request:#?}")
        }
    } else {
        panic!("Invalid request shouldn't be successful");
    }

    if let Err(failed_feed_request) = lore_api_client.request_patch_feed("amd-gfx", 300000) {
        match failed_feed_request {
            FailedFeedRequest::EndOfFeed => (),
            _ => panic!("Out-of-bounds request should return end of feed.\n{failed_feed_request:#?}")
        }
    } else {
        panic!("Out-of-bounds request shouldn't be successful");
    }
}

#[test]
#[ignore = "network-io"]
fn blocking_client_can_request_valid_available_lists() {
    let lore_api_client = BlockingLoreAPIClient::new();

    if let Err(_) = lore_api_client.request_available_lists(0) {
        panic!("Valid request should be successful");
    }
}

#[test]
#[ignore = "network-io"]
fn blocking_client_can_request_valid_patch_html() {
    let lore_api_client = BlockingLoreAPIClient::new();

    if let Err(_) = lore_api_client.request_patch_html("all", "Pine.LNX.4.58.0507282031180.3307@g5.osdl.org") {
        panic!("Valid request should be successful");
    }
}
