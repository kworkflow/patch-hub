use std::fmt::Display;

use color_eyre::eyre::{bail, Result};
use reqwest::blocking::Response;
use reqwest::Error;

#[cfg(test)]
mod tests;

const LORE_DOMAIN: &str = r"https://lore.kernel.org";
const BASE_QUERY_FOR_FEED_REQUEST: &str = r"?x=A&q=((s:patch+OR+s:rfc)+AND+NOT+s:re:)";

#[derive(Debug)]
pub enum FailedFeedRequest {
    UnknownError(Error),
    StatusNotOk(Response),
    EndOfFeed,
}

impl Display for FailedFeedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FailedFeedRequest::UnknownError(error) => write!(f, "Unknown error: {}", error),
            FailedFeedRequest::StatusNotOk(response) => write!(f, "Status not OK: {}", response.status()),
            FailedFeedRequest::EndOfFeed => write!(f, "End of feed"),
        }
    }
}

pub struct BlockingLoreAPIClient {}

impl BlockingLoreAPIClient {
    pub fn new() -> BlockingLoreAPIClient {
        BlockingLoreAPIClient {}
    }
}

pub trait PatchFeedRequest {
    fn request_patch_feed(self: &Self, target_list: &str, min_index: u32) -> Result<String>;
}

impl PatchFeedRequest for BlockingLoreAPIClient {
    fn request_patch_feed(self: &Self, target_list: &str, min_index: u32) -> Result<String> {
        let feed_request: String;
        let feed_response: Response;
        let feed_response_body: String;
        
        feed_request = format!("{LORE_DOMAIN}/{target_list}/{BASE_QUERY_FOR_FEED_REQUEST}&o={min_index}");

        match reqwest::blocking::get(feed_request) {
            Ok(response) => feed_response = response,
            Err(error) =>  bail!(FailedFeedRequest::UnknownError(error)),
        };

        match feed_response.status().as_u16() {
            200 => (),
            _ => bail!(FailedFeedRequest::StatusNotOk(feed_response)),
        };

        feed_response_body = feed_response.text()?;
        if feed_response_body.eq(r"</feed>") {
            bail!(FailedFeedRequest::EndOfFeed);
        };

        Ok(feed_response_body)
    }
}

#[derive(Debug)]
pub enum FailedAvailableListsRequest {
    UnknownError(Error),
    StatusNotOk(Response),
}

pub trait AvailableListsRequest {
    fn request_available_lists(self: &Self, min_index: u32) -> Result<String, FailedAvailableListsRequest>;
}

impl AvailableListsRequest for BlockingLoreAPIClient {
    fn request_available_lists(self: &Self, min_index: u32) -> Result<String, FailedAvailableListsRequest> {
        let available_lists_request: String;
        let available_lists: Response;
        
        available_lists_request = format!("{LORE_DOMAIN}/?&o={min_index}");

        match reqwest::blocking::get(available_lists_request) {
            Ok(response) => available_lists = response,
            Err(error) =>  return Err(FailedAvailableListsRequest::UnknownError(error)),
        };

        match available_lists.status().as_u16() {
            200 => (),
            _ => return Err(FailedAvailableListsRequest::StatusNotOk(available_lists)),
        };

        Ok(available_lists.text().unwrap())
    }
}

#[derive(Debug)]
pub enum FailedPatchHTMLRequest {
    UnknownError(Error),
    StatusNotOk(Response),
}

pub trait PatchHTMLRequest {
    fn request_patch_html(self: &Self, target_list: &str, message_id: &str) -> Result<String, FailedPatchHTMLRequest>;
}

impl PatchHTMLRequest for BlockingLoreAPIClient {
    fn request_patch_html(self: &Self, target_list: &str, message_id: &str) -> Result<String, FailedPatchHTMLRequest> {
        let patch_html_request: String;
        let patch_html: Response;

        patch_html_request = format!("{LORE_DOMAIN}/{target_list}/{message_id}/");

        match reqwest::blocking::get(patch_html_request) {
            Ok(response) => patch_html = response,
            Err(error) =>  return Err(FailedPatchHTMLRequest::UnknownError(error)),
        };

        match patch_html.status().as_u16() {
            200 => (),
            _ => return Err(FailedPatchHTMLRequest::StatusNotOk(patch_html)),
        };

        Ok(patch_html.text().unwrap())
    }
}
