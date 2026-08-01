use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, LazyLock},
};

use regex::Regex;

use crate::{
    infrastructure::{
        file_system::FileSystemTrait,
        shell::{ShellCommand, ShellTrait},
    },
    lore::{
        application::{
            api::LoreServiceApi,
            dto::{PatchTagSummary, PatchsetDetails},
            errors::LoreError,
        },
        domain::{
            mailing_list::MailingList,
            patch::{Author, Patch},
            patchset::PatchFeedIndex,
        },
        infrastructure::{
            http_lore_client::{FeedGateway, ListsGateway, LoreHttpError, PatchHtmlGateway},
            parsers,
            patchset_fetcher::PatchsetFetcher,
            patchset_parser::{self, PatchsetParser},
            persistence::LorePersistence,
        },
    },
};

pub struct LoreService {
    lists_gateway: Arc<dyn ListsGateway>,
    feed_gateway: Arc<dyn FeedGateway>,
    patch_html_gateway: Arc<dyn PatchHtmlGateway>,
    persistence: Arc<dyn LorePersistence>,
    patchset_fetcher: Arc<dyn PatchsetFetcher>,
    patchset_parser: Arc<dyn PatchsetParser>,
    fs: Arc<dyn FileSystemTrait>,
    shell: Arc<dyn ShellTrait>,
    feed_index_by_list: HashMap<String, PatchFeedIndex>,
}

impl LoreService {
    pub fn new(
        lists_gateway: Arc<dyn ListsGateway>,
        feed_gateway: Arc<dyn FeedGateway>,
        patch_html_gateway: Arc<dyn PatchHtmlGateway>,
        persistence: Arc<dyn LorePersistence>,
        patchset_fetcher: Arc<dyn PatchsetFetcher>,
        patchset_parser: Arc<dyn PatchsetParser>,
        fs: Arc<dyn FileSystemTrait>,
        shell: Arc<dyn ShellTrait>,
    ) -> Self {
        LoreService {
            lists_gateway,
            feed_gateway,
            patch_html_gateway,
            persistence,
            patchset_fetcher,
            patchset_parser,
            fs,
            shell,
            feed_index_by_list: HashMap::new(),
        }
    }
}

impl LoreServiceApi for LoreService {
    fn load_available_lists(&self) -> Result<Vec<MailingList>, LoreError> {
        Ok(self.persistence.load_available_lists()?)
    }

    fn refresh_available_lists(&self) -> Result<Vec<MailingList>, LoreError> {
        const LORE_PAGE_SIZE: usize = 200;

        let gateway = Arc::clone(&self.lists_gateway);
        let persistence = Arc::clone(&self.persistence);

        let mut all_lists: Vec<MailingList> = Vec::new();
        let mut offset = 0;

        loop {
            let body = gateway
                .fetch_available_lists_page(offset)
                .map_err(LoreError::Http)?;

            let page = parsers::parse_available_lists(&body);
            if page.is_empty() {
                break;
            }

            all_lists.extend(page);
            offset += LORE_PAGE_SIZE;
        }

        all_lists.sort();
        persistence.save_available_lists(&all_lists)?;
        Ok(all_lists)
    }

    fn load_bookmarked_patchsets(&self) -> Result<Vec<Patch>, LoreError> {
        Ok(self.persistence.load_bookmarked_patchsets()?)
    }

    fn save_bookmarked_patchsets(&self, patchsets: &[Patch]) -> Result<(), LoreError> {
        Ok(self.persistence.save_bookmarked_patchsets(patchsets)?)
    }

    fn load_reviewed_patchsets(&self) -> Result<HashMap<String, HashSet<usize>>, LoreError> {
        Ok(self.persistence.load_reviewed_patchsets()?)
    }

    fn save_reviewed_patchsets(
        &self,
        reviewed: &HashMap<String, HashSet<usize>>,
    ) -> Result<(), LoreError> {
        Ok(self.persistence.save_reviewed_patchsets(reviewed)?)
    }

    fn fetch_next_patch_page(
        &mut self,
        target_list: &str,
        page_size: usize,
        page_number: usize,
    ) -> Result<Vec<Patch>, LoreError> {
        let needed = page_size * page_number;

        self.feed_index_by_list
            .entry(target_list.to_string())
            .or_insert_with(|| PatchFeedIndex::new(target_list.to_string()));

        // Clone the Arc so the borrow on `self.feed_gateway` doesn't conflict
        // with the mutable borrow on `self.feed_index_by_list`.
        let gateway = Arc::clone(&self.feed_gateway);

        loop {
            let current = self.feed_index_by_list[target_list]
                .representative_patch_ids()
                .len();
            if current >= needed {
                break;
            }

            let offset = self.feed_index_by_list[target_list].next_offset();
            match gateway.fetch_patch_feed_page(target_list, offset) {
                Ok(body) => {
                    let feed = parsers::parse_patch_feed(&body).map_err(LoreError::Parse)?;
                    let index = self.feed_index_by_list.get_mut(target_list).unwrap();
                    index.process_feed_page(feed);
                    index.advance_offset();
                }
                Err(LoreHttpError::EndOfFeed) => break,
                Err(e) => return Err(LoreError::Http(e)),
            }
        }

        match self.feed_index_by_list[target_list].get_page(page_size, page_number) {
            Some(patches) => Ok(patches.into_iter().cloned().collect()),
            None => Err(LoreError::EndOfFeed),
        }
    }

    fn reset_feed_cursor(&mut self, target_list: &str) {
        self.feed_index_by_list.remove(target_list);
    }

    fn fetch_patchset_details(
        &self,
        representative_patch: &Patch,
    ) -> Result<PatchsetDetails, LoreError> {
        let patchset_path = self
            .patchset_fetcher
            .download(representative_patch)
            .map_err(|e| LoreError::PatchNotFound(e.to_string()))?;

        let raw_patches = self
            .patchset_parser
            .split_patchset(&patchset_path)
            .map_err(LoreError::Parse)?;

        let tag_summary = raw_patches.iter().map(|p| extract_tag_summary(p)).collect();

        Ok(PatchsetDetails {
            representative_patch: representative_patch.clone(),
            patchset_path,
            raw_patches,
            tag_summary,
        })
    }

    fn prepare_reply_commands(
        &self,
        tmp_dir: &Path,
        target_list: &str,
        patches: &[String],
        patches_to_reply: &[bool],
        git_signature: &str,
        git_send_email_options: &str,
    ) -> Result<Vec<ShellCommand>, LoreError> {
        static RE_MESSAGE_ID: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?m)^Message-Id: <(.*?)>").unwrap());

        let gateway = Arc::clone(&self.patch_html_gateway);
        let mut commands = Vec::new();

        for (i, patch) in patches.iter().enumerate() {
            if !patches_to_reply[i] {
                continue;
            }

            let message_id = RE_MESSAGE_ID
                .captures(patch)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str())
                .ok_or_else(|| LoreError::Parse("Message-Id header not found".to_string()))?;

            let reply_path = tmp_dir.join(format!("{message_id}-reply.mbx"));
            let mut reply = patchset_parser::generate_reply_template(patch);
            reply.push_str(&format!("\nReviewed-by: {git_signature}\n"));
            self.fs
                .write(&reply_path, reply.as_bytes())
                .map_err(LoreError::Persistence)?;

            let patch_html = gateway
                .fetch_patch_html(target_list, message_id)
                .map_err(LoreError::Http)?;

            let command = patchset_parser::extract_git_reply_command(
                &patch_html,
                git_send_email_options,
                &format!("{}", reply_path.display()),
            );
            commands.push(command);
        }

        Ok(commands)
    }

    fn get_git_signature(&self, git_repo_path: &str) -> (String, String) {
        let mut name_args = vec!["config".to_string(), "user.name".to_string()];
        let mut email_args = vec!["config".to_string(), "user.email".to_string()];

        if !git_repo_path.is_empty() {
            name_args.insert(0, git_repo_path.to_string());
            name_args.insert(0, "-C".to_string());
            email_args.insert(0, git_repo_path.to_string());
            email_args.insert(0, "-C".to_string());
        }

        let name_cmd = ShellCommand {
            program: "git".to_string(),
            args: name_args,
        };
        let email_cmd = ShellCommand {
            program: "git".to_string(),
            args: email_args,
        };

        let name = self
            .shell
            .execute(&name_cmd)
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default();

        let email = self
            .shell
            .execute(&email_cmd)
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default();

        (name, email)
    }
}

fn extract_tag_summary(raw_patch: &str) -> PatchTagSummary {
    let (cover, _) = patchset_parser::split_cover(raw_patch);

    let mut reviewed_by = HashSet::new();
    let mut tested_by = HashSet::new();
    let mut acked_by = HashSet::new();

    for line in cover.lines() {
        let line = line.trim_start();
        for (prefix, set) in [
            ("Reviewed-by:", &mut reviewed_by),
            ("Tested-by:", &mut tested_by),
            ("Acked-by:", &mut acked_by),
        ] {
            if let Some(rest) = line.strip_prefix(prefix) {
                let parts: Vec<&str> = rest.trim().split('<').collect();
                if parts.len() == 2 {
                    let name = parts[0].trim().to_string();
                    let email = parts[1].trim_end_matches('>').trim().to_string();
                    set.insert(Author { name, email });
                }
                break;
            }
        }
    }

    PatchTagSummary {
        reviewed_by,
        tested_by,
        acked_by,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc};

    use crate::lore::infrastructure::{
        http_lore_client::{
            LoreHttpError, MockFeedGateway, MockListsGateway, MockPatchHtmlGateway,
        },
        patchset_fetcher::MockPatchsetFetcher,
        patchset_parser::MockPatchsetParser,
        persistence::MockLorePersistence,
    };
    use crate::{
        infrastructure::{file_system::MockFileSystemTrait, shell::MockShellTrait},
        lore::domain::mailing_list::MailingList,
    };

    use super::*;

    fn make_service(
        lists_gateway: MockListsGateway,
        feed_gateway: MockFeedGateway,
        patch_html_gateway: MockPatchHtmlGateway,
        persistence: MockLorePersistence,
        fetcher: MockPatchsetFetcher,
        parser: MockPatchsetParser,
    ) -> LoreService {
        LoreService::new(
            Arc::new(lists_gateway),
            Arc::new(feed_gateway),
            Arc::new(patch_html_gateway),
            Arc::new(persistence),
            Arc::new(fetcher),
            Arc::new(parser),
            Arc::new(MockFileSystemTrait::new()),
            Arc::new(MockShellTrait::new()),
        )
    }

    #[test]
    fn load_available_lists_delegates_to_persistence() {
        let mut persistence = MockLorePersistence::new();
        persistence
            .expect_load_available_lists()
            .times(1)
            .returning(|| Ok(vec![MailingList::new("linux-mm", "desc")]));

        let svc = make_service(
            MockListsGateway::new(),
            MockFeedGateway::new(),
            MockPatchHtmlGateway::new(),
            persistence,
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        let result = svc.load_available_lists().unwrap();
        assert_eq!(1, result.len());
        assert_eq!("linux-mm", result[0].name());
    }

    #[test]
    fn refresh_available_lists_paginates_and_sorts() {
        let mut lists_gateway = MockListsGateway::new();
        lists_gateway
            .expect_fetch_available_lists_page()
            .withf(|offset| *offset == 0)
            .times(1)
            .returning(|_| {
                Ok(fs::read_to_string(
                    "test_samples/lore_session/process_available_lists/available_lists_response-1.html",
                )
                .unwrap())
            });
        lists_gateway
            .expect_fetch_available_lists_page()
            .withf(|offset| *offset == 200)
            .times(1)
            .returning(|_| {
                Ok(fs::read_to_string(
                    "test_samples/lore_session/process_available_lists/available_lists_response-2.html",
                )
                .unwrap())
            });
        lists_gateway
            .expect_fetch_available_lists_page()
            .withf(|offset| *offset == 400)
            .times(1)
            .returning(|_| {
                Ok(fs::read_to_string(
                    "test_samples/lore_session/process_available_lists/available_lists_response-3.html",
                )
                .unwrap())
            });

        let mut persistence = MockLorePersistence::new();
        persistence
            .expect_save_available_lists()
            .times(1)
            .returning(|_| Ok(()));

        let svc = make_service(
            lists_gateway,
            MockFeedGateway::new(),
            MockPatchHtmlGateway::new(),
            persistence,
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        let lists = svc.refresh_available_lists().unwrap();
        assert_eq!(320, lists.len());
        assert_eq!("accel-config", lists[0].name());
        assert_eq!("yocto-toaster", lists[319].name());
    }

    #[test]
    fn fetch_next_patch_page_returns_patches() {
        let src = "test_samples/lore_session/process_representative_patch/patch_feed_sample_1.xml";
        let target = "some-list";

        let mut feed_gateway = MockFeedGateway::new();
        feed_gateway
            .expect_fetch_patch_feed_page()
            .withf(move |list, offset| list == target && *offset == 0)
            .times(1)
            .returning(move |_, _| Ok(fs::read_to_string(src).unwrap()));

        let mut svc = make_service(
            MockListsGateway::new(),
            feed_gateway,
            MockPatchHtmlGateway::new(),
            MockLorePersistence::new(),
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        let patches = svc.fetch_next_patch_page(target, 1, 1).unwrap();
        assert_eq!(1, patches.len());
        assert!(patches[0]
            .message_id()
            .href
            .contains("1234.567-1-john@johnson.com"));
    }

    #[test]
    fn fetch_next_patch_page_returns_end_of_feed() {
        let mut feed_gateway = MockFeedGateway::new();
        feed_gateway
            .expect_fetch_patch_feed_page()
            .times(1)
            .returning(|_, _| Err(LoreHttpError::EndOfFeed));

        let mut svc = make_service(
            MockListsGateway::new(),
            feed_gateway,
            MockPatchHtmlGateway::new(),
            MockLorePersistence::new(),
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        let result = svc.fetch_next_patch_page("some-list", 1, 1);
        assert!(matches!(result, Err(LoreError::EndOfFeed)));
    }

    #[test]
    fn reset_feed_cursor_clears_index() {
        let src = "test_samples/lore_session/process_representative_patch/patch_feed_sample_1.xml";
        let target = "some-list";

        let mut feed_gateway = MockFeedGateway::new();
        // First call: returns page with 1 patch
        feed_gateway
            .expect_fetch_patch_feed_page()
            .returning(move |_, _| Ok(fs::read_to_string(src).unwrap()));

        let mut svc = make_service(
            MockListsGateway::new(),
            feed_gateway,
            MockPatchHtmlGateway::new(),
            MockLorePersistence::new(),
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        svc.fetch_next_patch_page(target, 1, 1).unwrap();
        assert!(svc.feed_index_by_list.contains_key(target));

        svc.reset_feed_cursor(target);
        assert!(!svc.feed_index_by_list.contains_key(target));
    }
}
