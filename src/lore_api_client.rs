use reqwest::blocking::Response;
use reqwest::Error;

const LORE_DOMAIN: &str = r"https://lore.kernel.org/";
const BASE_QUERY_FOR_FEED_REQUEST: &str = r"/?x=A&q=((s:patch+OR+s:rfc)+AND+NOT+s:re:)&o=";

pub enum FailedFeedResquest {
    UnknowError(Error),
    StatusNotOk(Response),
    EndOfFeed,
}

pub struct LoreAPIClient {}

impl LoreAPIClient {
    pub fn request_patch_feed(target_list: &String, min_index: u32) -> Result<String, FailedFeedResquest> {
        let mut feed_request: String = String::from(LORE_DOMAIN);
        let feed_response: Response;
        let feed_response_body: String;
        
        feed_request.push_str(target_list);
        feed_request.push_str(BASE_QUERY_FOR_FEED_REQUEST);
        feed_request.push_str(&min_index.to_string());

        match reqwest::blocking::get(feed_request) {
            Ok(response) => feed_response = response,
            Err(error) =>  return Err(FailedFeedResquest::UnknowError(error)),
        };

        match feed_response.status().as_u16() {
            200 => (),
            _ => return Err(FailedFeedResquest::StatusNotOk(feed_response)),
        };

        feed_response_body = feed_response.text().unwrap();
        if feed_response_body.eq(r"</feed>") {
            return Err(FailedFeedResquest::EndOfFeed);
        };

        Ok(feed_response_body)
    }
}