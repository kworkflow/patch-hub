use mockall::automock;
use reqwest::{
    blocking::{RequestBuilder as BlockingRequestBuilder, Response},
    Method, StatusCode,
};
use thiserror::Error;

#[cfg(test)]
mod tests;

const LORE_DOMAIN: &str = r"https://lore.kernel.org";
const BASE_QUERY_FOR_FEED_REQUEST: &str = r"?x=A&q=((s:patch+OR+s:rfc)+AND+NOT+s:re:)";

#[derive(Error, Debug)]
pub enum ClientError {
    #[error(transparent)]
    FromReqwest(#[from] reqwest::Error),

    #[error("Got response with status {0}: {1:?}")]
    UnexpectedResponse(StatusCode, Response),

    #[error("Feed ended")]
    EndOfFeed,
}

pub struct BlockingLoreAPIClient {
    pub lore_domain: String,
    blocking_client: reqwest::blocking::Client,
}

impl Default for BlockingLoreAPIClient {
    fn default() -> Self {
        let blocking_client = reqwest::blocking::Client::new();
        Self::new(blocking_client)
    }
}

impl BlockingLoreAPIClient {
    pub fn new(blocking_client: reqwest::blocking::Client) -> BlockingLoreAPIClient {
        BlockingLoreAPIClient {
            lore_domain: LORE_DOMAIN.to_string(),
            blocking_client,
        }
    }
    pub fn request_and_get_body(
        &self,
        request_builder: BlockingRequestBuilder,
    ) -> Result<String, ClientError> {
        let response = request_builder.send()?;

        let response_status = response.status();
        let StatusCode::OK = response_status else {
            return Err(ClientError::UnexpectedResponse(response_status, response));
        };

        let body = response.text()?;

        Ok(body)
    }
}

#[automock]
pub trait PatchFeedRequest {
    fn request_patch_feed(
        &self,
        target_list: &str,
        min_index: usize,
    ) -> Result<String, ClientError>;
}

impl PatchFeedRequest for BlockingLoreAPIClient {
    fn request_patch_feed(
        &self,
        target_list: &str,
        min_index: usize,
    ) -> Result<String, ClientError> {
        let feed_url: String = format!(
            "{}/{target_list}/{BASE_QUERY_FOR_FEED_REQUEST}&o={min_index}",
            self.lore_domain
        );

        let request_builder = self.blocking_client.request(Method::GET, feed_url);

        let feed_response_body = self.request_and_get_body(request_builder)?;

        if feed_response_body.eq(r"</feed>") {
            return Err(ClientError::EndOfFeed);
        };

        Ok(feed_response_body)
    }
}

#[automock]
pub trait AvailableListsRequest {
    fn request_available_lists(&self, min_index: usize) -> Result<String, ClientError>;
}

impl AvailableListsRequest for BlockingLoreAPIClient {
    fn request_available_lists(&self, min_index: usize) -> Result<String, ClientError> {
        let available_lists_url = format!("{}/?&o={min_index}", self.lore_domain);

        let request_builder = self
            .blocking_client
            .request(Method::GET, available_lists_url);

        self.request_and_get_body(request_builder)
    }
}

#[automock]
pub trait PatchHTMLRequest {
    fn request_patch_html(
        &self,
        target_list: &str,
        message_id: &str,
    ) -> Result<String, ClientError>;
}

impl PatchHTMLRequest for BlockingLoreAPIClient {
    fn request_patch_html(
        &self,
        target_list: &str,
        message_id: &str,
    ) -> Result<String, ClientError> {
        let patch_html_url = format!("{}/{target_list}/{message_id}/", self.lore_domain);

        let request_builder = self.blocking_client.request(Method::GET, patch_html_url);

        self.request_and_get_body(request_builder)
    }
}
