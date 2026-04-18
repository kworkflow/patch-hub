use mockall::automock;
use thiserror::Error;

use std::sync::Arc;

use crate::infrastructure::net::{HttpMethod, NetClientTrait, NetError};

#[cfg(test)]
mod tests;

const LORE_DOMAIN: &str = r"https://lore.kernel.org";
const BASE_QUERY_FOR_FEED_REQUEST: &str = r"?x=A&q=((s:patch+OR+s:rfc)+AND+NOT+s:re:)";

#[derive(Error, Debug)]
pub enum ClientError {
    #[error(transparent)]
    Net(#[from] NetError),

    #[error("Feed ended")]
    EndOfFeed,
}

#[derive(Clone)]
pub struct BlockingLoreAPIClient {
    pub lore_domain: String,
    net_client: Arc<dyn NetClientTrait>,
}

impl BlockingLoreAPIClient {
    pub fn new(net_client: Box<dyn NetClientTrait>) -> BlockingLoreAPIClient {
        BlockingLoreAPIClient {
            lore_domain: LORE_DOMAIN.to_string(),
            net_client: Arc::from(net_client),
        }
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
        let url = format!(
            "{}/{target_list}/{BASE_QUERY_FOR_FEED_REQUEST}&o={min_index}",
            self.lore_domain
        );

        let body = self.net_client.request(HttpMethod::Get, &url)?;

        if body.eq(r"</feed>") {
            return Err(ClientError::EndOfFeed);
        }

        Ok(body)
    }
}

#[automock]
pub trait AvailableListsRequest {
    fn request_available_lists(&self, min_index: usize) -> Result<String, ClientError>;
}

impl AvailableListsRequest for BlockingLoreAPIClient {
    fn request_available_lists(&self, min_index: usize) -> Result<String, ClientError> {
        let url = format!("{}/?&o={min_index}", self.lore_domain);
        Ok(self.net_client.request(HttpMethod::Get, &url)?)
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
        let url = format!("{}/{target_list}/{message_id}/", self.lore_domain);
        Ok(self.net_client.request(HttpMethod::Get, &url)?)
    }
}

mockall::mock! {
    pub BlockingLoreAPIClient {}
    impl PatchFeedRequest for BlockingLoreAPIClient {
        fn request_patch_feed(
                    &self,
                    target_list: &str,
                    min_index: usize,
                ) -> Result<String, ClientError>;
    }
    impl AvailableListsRequest for BlockingLoreAPIClient {
        fn request_available_lists(
            &self,
            min_index: usize,
        ) -> Result<String, ClientError>;
    }
    impl PatchHTMLRequest for BlockingLoreAPIClient {
        fn request_patch_html(
            &self,
            _target_list: &str,
            message_id: &str,
        ) -> Result<String, ClientError>;
    }
}
