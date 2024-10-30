use super::*;
use crate::lore::patch::PatchFeed;

#[test]
#[ignore = "network-io"]
fn blocking_client_can_request_valid_patch_feed() {
    let lore_api_client = BlockingLoreAPIClient::default();

    let patch_feed = lore_api_client.request_patch_feed("amd-gfx", 0).unwrap();
    let patch_feed: PatchFeed = serde_xml_rs::from_str(&patch_feed).unwrap();
    let patches = patch_feed.patches();

    assert_eq!(
        200,
        patches.len(),
        "Should successfully request patch feed with 200 patches"
    );
}

#[test]
#[ignore = "network-io"]
fn blocking_client_should_detect_failed_patch_feed_request() {
    let lore_api_client = BlockingLoreAPIClient::default();

    if let Err(client_error) = lore_api_client.request_patch_feed("invalid-list", 0) {
        match client_error {
            ClientError::FromUreq(_) => (),
            _ => {
                panic!("Invalid request should return non 200 OK status.\n{client_error:#?}")
            }
        }
    } else {
        panic!("Invalid request shouldn't be successful");
    }

    if let Err(client_error) = lore_api_client.request_patch_feed("amd-gfx", 300000) {
        match client_error {
            ClientError::EndOfFeed => (),
            _ => {
                panic!("Out-of-bounds request should return end of feed.\n{client_error:#?}")
            }
        }
    } else {
        panic!("Out-of-bounds request shouldn't be successful");
    }
}

#[test]
#[ignore = "network-io"]
fn blocking_client_can_request_valid_available_lists() {
    let lore_api_client = BlockingLoreAPIClient::default();

    if lore_api_client.request_available_lists(0).is_err() {
        panic!("Valid request should be successful");
    }
}

#[test]
#[ignore = "network-io"]
fn blocking_client_can_request_valid_patch_html() {
    let lore_api_client = BlockingLoreAPIClient::default();

    if lore_api_client
        .request_patch_html("all", "Pine.LNX.4.58.0507282031180.3307@g5.osdl.org")
        .is_err()
    {
        panic!("Valid request should be successful");
    }
}
