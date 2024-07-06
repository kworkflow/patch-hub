use reqwest::blocking::Response;
use reqwest::Error;

const LORE_DOMAIN: &str = r"https://lore.kernel.org";
const BASE_QUERY_FOR_FEED_REQUEST: &str = r"?x=A&q=((s:patch+OR+s:rfc)+AND+NOT+s:re:)";

pub enum FailedFeedRequest {
    UnknownError(Error),
    StatusNotOk(Response),
    EndOfFeed,
}

pub struct BlockingLoreAPIClient {}

impl BlockingLoreAPIClient {
    pub fn new() -> BlockingLoreAPIClient {
        BlockingLoreAPIClient {}
    }
}

pub trait PatchFeedRequest {
    fn request_patch_feed(self: &Self, target_list: &String, min_index: u32) -> Result<String, FailedFeedRequest>;
}

impl PatchFeedRequest for BlockingLoreAPIClient {
    fn request_patch_feed(self: &Self, target_list: &String, min_index: u32) -> Result<String, FailedFeedRequest> {
        let feed_request: String;
        let feed_response: Response;
        let feed_response_body: String;
        
        feed_request = format!("{LORE_DOMAIN}/{target_list}/{BASE_QUERY_FOR_FEED_REQUEST}&o={min_index}");

        match reqwest::blocking::get(feed_request) {
            Ok(response) => feed_response = response,
            Err(error) =>  return Err(FailedFeedRequest::UnknownError(error)),
        };

        match feed_response.status().as_u16() {
            200 => (),
            _ => return Err(FailedFeedRequest::StatusNotOk(feed_response)),
        };

        feed_response_body = feed_response.text().unwrap();
        if feed_response_body.eq(r"</feed>") {
            return Err(FailedFeedRequest::EndOfFeed);
        };

        Ok(feed_response_body)
    }
}