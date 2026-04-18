#![allow(dead_code)]

use mockall::automock;
use thiserror::Error;

use std::sync::Arc;

use crate::infrastructure::net::{HttpMethod, NetClientTrait, NetError};

const LORE_DOMAIN: &str = "https://lore.kernel.org";
const BASE_QUERY_FOR_FEED_REQUEST: &str = "?x=A&q=((s:patch+OR+s:rfc)+AND+NOT+s:re:)";

#[derive(Debug, Error)]
pub enum LoreHttpError {
    #[error(transparent)]
    Net(#[from] NetError),

    #[error("feed ended")]
    EndOfFeed,
}

#[automock]
pub trait ListsGateway: Send + Sync {
    fn fetch_available_lists_page(&self, offset: usize) -> Result<String, LoreHttpError>;
}

#[automock]
pub trait FeedGateway: Send + Sync {
    fn fetch_patch_feed_page(
        &self,
        target_list: &str,
        offset: usize,
    ) -> Result<String, LoreHttpError>;
}

#[automock]
pub trait PatchHtmlGateway: Send + Sync {
    fn fetch_patch_html(
        &self,
        target_list: &str,
        message_id: &str,
    ) -> Result<String, LoreHttpError>;
}

pub struct HttpLoreGateway {
    client: Arc<dyn NetClientTrait>,
    lore_domain: String,
}

impl HttpLoreGateway {
    pub fn new(client: Arc<dyn NetClientTrait>) -> Self {
        HttpLoreGateway {
            client,
            lore_domain: LORE_DOMAIN.to_string(),
        }
    }
}

impl ListsGateway for HttpLoreGateway {
    fn fetch_available_lists_page(&self, offset: usize) -> Result<String, LoreHttpError> {
        let url = format!("{}/?&o={offset}", self.lore_domain);
        Ok(self.client.request(HttpMethod::Get, &url)?)
    }
}

impl FeedGateway for HttpLoreGateway {
    fn fetch_patch_feed_page(
        &self,
        target_list: &str,
        offset: usize,
    ) -> Result<String, LoreHttpError> {
        let url = format!(
            "{}/{target_list}/{BASE_QUERY_FOR_FEED_REQUEST}&o={offset}",
            self.lore_domain
        );

        let body = self.client.request(HttpMethod::Get, &url)?;

        if body.eq("</feed>") {
            return Err(LoreHttpError::EndOfFeed);
        }

        Ok(body)
    }
}

impl PatchHtmlGateway for HttpLoreGateway {
    fn fetch_patch_html(
        &self,
        target_list: &str,
        message_id: &str,
    ) -> Result<String, LoreHttpError> {
        let url = format!("{}/{target_list}/{message_id}/", self.lore_domain);
        Ok(self.client.request(HttpMethod::Get, &url)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::net::MockNetClientTrait;

    #[test]
    fn fetch_available_lists_page_builds_correct_url() {
        let mut mock = MockNetClientTrait::new();
        mock.expect_request()
            .withf(|_, url| url == "https://lore.kernel.org/?&o=0")
            .times(1)
            .returning(|_, _| Ok("<html>content</html>".to_string()));

        let gw = HttpLoreGateway::new(Arc::new(mock));
        assert!(gw.fetch_available_lists_page(0).is_ok());
    }

    #[test]
    fn fetch_available_lists_page_builds_url_with_offset() {
        let mut mock = MockNetClientTrait::new();
        mock.expect_request()
            .withf(|_, url| url == "https://lore.kernel.org/?&o=200")
            .times(1)
            .returning(|_, _| Ok("<html>content</html>".to_string()));

        let gw = HttpLoreGateway::new(Arc::new(mock));
        assert!(gw.fetch_available_lists_page(200).is_ok());
    }

    #[test]
    fn fetch_patch_feed_page_builds_correct_url() {
        let expected_url =
            "https://lore.kernel.org/linux-kernel/?x=A&q=((s:patch+OR+s:rfc)+AND+NOT+s:re:)&o=0";
        let mut mock = MockNetClientTrait::new();
        mock.expect_request()
            .withf(move |_, url| url == expected_url)
            .times(1)
            .returning(|_, _| Ok("<feed>data</feed>".to_string()));

        let gw = HttpLoreGateway::new(Arc::new(mock));
        assert!(gw.fetch_patch_feed_page("linux-kernel", 0).is_ok());
    }

    #[test]
    fn fetch_patch_feed_page_returns_end_of_feed_on_empty_body() {
        let mut mock = MockNetClientTrait::new();
        mock.expect_request()
            .times(1)
            .returning(|_, _| Ok("</feed>".to_string()));

        let gw = HttpLoreGateway::new(Arc::new(mock));
        let result = gw.fetch_patch_feed_page("linux-kernel", 0);
        assert!(matches!(result, Err(LoreHttpError::EndOfFeed)));
    }

    #[test]
    fn fetch_patch_html_builds_correct_url() {
        let expected_url = "https://lore.kernel.org/linux-mm/abc123@host.example/";
        let mut mock = MockNetClientTrait::new();
        mock.expect_request()
            .withf(move |_, url| url == expected_url)
            .times(1)
            .returning(|_, _| Ok("<html>patch</html>".to_string()));

        let gw = HttpLoreGateway::new(Arc::new(mock));
        assert!(gw
            .fetch_patch_html("linux-mm", "abc123@host.example")
            .is_ok());
    }
}
