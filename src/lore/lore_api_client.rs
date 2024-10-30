use std::time::Duration;

use mockall::automock;
use thiserror::Error;
use ureq::tls::TlsConfig;
use ureq::Agent;

#[cfg(test)]
mod tests;

const LORE_DOMAIN: &str = r"https://lore.kernel.org";
const BASE_QUERY_FOR_FEED_REQUEST: &str = r"?x=A&q=((s:patch+OR+s:rfc)+AND+NOT+s:re:)";

#[derive(Error, Debug)]
pub enum ClientError {
    #[error(transparent)]
    FromUreq(#[from] ureq::Error),

    #[error("Got response with status {0}: {1:?}")]
    UnexpectedResponse(u16, String),

    #[error("Feed ended")]
    EndOfFeed,
}

#[derive(Clone)]
pub struct BlockingLoreAPIClient {
    pub lore_domain: String,
    client: ureq::Agent,
}
impl Default for BlockingLoreAPIClient {
    fn default() -> Self {
        let agent: Agent = Agent::config_builder()
            .user_agent(Some(format!(
                "kworkflow/patch-hub/{}",
                env!("CARGO_PKG_VERSION")
            )))
            .timeout_per_call(Some(Duration::from_secs(120)))
            .tls_config(TlsConfig::builder().build())
            .build()
            .into();
        Self::new(agent)
    }
}

impl BlockingLoreAPIClient {
    pub fn new(client: ureq::Agent) -> BlockingLoreAPIClient {
        BlockingLoreAPIClient {
            lore_domain: LORE_DOMAIN.to_string(),
            client,
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
        let feed_url: String = format!(
            "{}/{target_list}/{BASE_QUERY_FOR_FEED_REQUEST}&o={min_index}",
            self.lore_domain
        );

        let request_builder = self
            .client
            .get(feed_url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml");

        let feed_response_body = request_builder.call()?.body_mut().read_to_string()?;

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

        let body: String = ureq::get(&available_lists_url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml")
            .call()?
            .body_mut()
            .read_to_string()?;
        Ok(body)
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

        let body: String = ureq::get(&patch_html_url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml")
            .call()?
            .body_mut()
            .read_to_string()?;

        Ok(body)
    }
}
