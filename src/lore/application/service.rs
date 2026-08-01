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
            cache::{
                BootstrapLoreData, CacheMode, CacheTtl, FeedCacheEntry, LoreCache,
                MailingListsCacheEntry, PatchsetCacheEntry, PatchsetCacheKey,
            },
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
            persistence::{MailingListsCacheStore, UserLoreStateStore},
        },
    },
};

pub struct LoreService {
    lists_gateway: Arc<dyn ListsGateway>,
    feed_gateway: Arc<dyn FeedGateway>,
    patch_html_gateway: Arc<dyn PatchHtmlGateway>,
    lists_store: Arc<dyn MailingListsCacheStore>,
    user_state: Arc<dyn UserLoreStateStore>,
    patchset_fetcher: Arc<dyn PatchsetFetcher>,
    patchset_parser: Arc<dyn PatchsetParser>,
    fs: Arc<dyn FileSystemTrait>,
    shell: Arc<dyn ShellTrait>,
    cache: LoreCache,
    ttl: CacheTtl,
}

impl LoreService {
    pub fn new(
        lists_gateway: Arc<dyn ListsGateway>,
        feed_gateway: Arc<dyn FeedGateway>,
        patch_html_gateway: Arc<dyn PatchHtmlGateway>,
        lists_store: Arc<dyn MailingListsCacheStore>,
        user_state: Arc<dyn UserLoreStateStore>,
        patchset_fetcher: Arc<dyn PatchsetFetcher>,
        patchset_parser: Arc<dyn PatchsetParser>,
        fs: Arc<dyn FileSystemTrait>,
        shell: Arc<dyn ShellTrait>,
        ttl: CacheTtl,
    ) -> Self {
        LoreService {
            lists_gateway,
            feed_gateway,
            patch_html_gateway,
            lists_store,
            user_state,
            patchset_fetcher,
            patchset_parser,
            fs,
            shell,
            cache: LoreCache::new(),
            ttl,
        }
    }
}

impl LoreServiceApi for LoreService {
    fn fetch_available_lists(&mut self, mode: CacheMode) -> Result<Vec<MailingList>, LoreError> {
        const LORE_PAGE_SIZE: usize = 200;

        match mode {
            CacheMode::UseCache => {
                // 1. In-memory hit
                if let Some(entry) = &self.cache.lists {
                    if !entry.is_stale(self.ttl.mailing_lists) {
                        tracing::debug!("mailing lists cache: hit");
                        return Ok(entry.lists.clone());
                    }
                    tracing::debug!("mailing lists cache: stale, falling back to disk");
                    self.cache.lists = None;
                }
                // 2. Disk fallback
                match self.lists_store.load_available_lists() {
                    Ok(lists) => {
                        tracing::debug!("mailing lists cache: disk hit");
                        self.cache.lists = Some(MailingListsCacheEntry::new(lists.clone()));
                        return Ok(lists);
                    }
                    Err(e) => {
                        tracing::debug!(error = %e, "mailing lists cache: disk miss");
                        return Err(LoreError::Persistence(e));
                    }
                }
            }
            CacheMode::Refresh => {
                tracing::info!("mailing lists cache: refresh requested");
                self.cache.lists = None;
            }
            CacheMode::Bypass => {
                tracing::debug!("mailing lists cache: bypass");
            }
        }

        // Network fetch (Refresh or Bypass)
        let gateway = Arc::clone(&self.lists_gateway);
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

        match mode {
            CacheMode::Bypass => {}
            CacheMode::Refresh | CacheMode::UseCache => {
                self.lists_store.save_available_lists(&all_lists)?;
                self.cache.lists = Some(MailingListsCacheEntry::new(all_lists.clone()));
            }
        }

        Ok(all_lists)
    }

    fn load_bookmarked_patchsets(&self) -> Result<Vec<Patch>, LoreError> {
        Ok(self.user_state.load_bookmarked_patchsets()?)
    }

    fn save_bookmarked_patchsets(&self, patchsets: &[Patch]) -> Result<(), LoreError> {
        Ok(self.user_state.save_bookmarked_patchsets(patchsets)?)
    }

    fn load_reviewed_patchsets(&self) -> Result<HashMap<String, HashSet<usize>>, LoreError> {
        Ok(self.user_state.load_reviewed_patchsets()?)
    }

    fn save_reviewed_patchsets(
        &self,
        reviewed: &HashMap<String, HashSet<usize>>,
    ) -> Result<(), LoreError> {
        Ok(self.user_state.save_reviewed_patchsets(reviewed)?)
    }

    fn fetch_next_patch_page(
        &mut self,
        target_list: &str,
        page_size: usize,
        page_number: usize,
        mode: CacheMode,
    ) -> Result<Vec<Patch>, LoreError> {
        let needed = page_size * page_number;

        match mode {
            CacheMode::Refresh => {
                tracing::info!(list = target_list, "feed cache: refresh requested");
                self.cache.feeds.remove(target_list);
            }
            CacheMode::UseCache => {
                // Return from in-memory index if not stale and already sufficient.
                if let Some(entry) = self.cache.feeds.get(target_list) {
                    if entry.is_stale(self.ttl.feed) {
                        tracing::debug!(list = target_list, "feed cache: stale, evicting");
                        self.cache.feeds.remove(target_list);
                    } else {
                        let current = entry.index.representative_patch_ids().len();
                        if current >= needed {
                            tracing::debug!(list = target_list, "feed cache: hit");
                            return match entry.index.get_page(page_size, page_number) {
                                Some(patches) => Ok(patches.into_iter().cloned().collect()),
                                None => Err(LoreError::EndOfFeed),
                            };
                        }
                    }
                }
            }
            CacheMode::Bypass => {}
        }

        // Get-or-create the cache entry.
        self.cache
            .feeds
            .entry(target_list.to_string())
            .or_insert_with(|| FeedCacheEntry::new(PatchFeedIndex::new(target_list.to_string())));

        // Clone the Arc so the borrow on `self.feed_gateway` doesn't conflict
        // with the mutable borrow on `self.cache.feeds`.
        let gateway = Arc::clone(&self.feed_gateway);

        loop {
            let current = self.cache.feeds[target_list]
                .index
                .representative_patch_ids()
                .len();
            if current >= needed {
                break;
            }

            let offset = self.cache.feeds[target_list].index.next_offset();
            match gateway.fetch_patch_feed_page(target_list, offset) {
                Ok(body) => {
                    let feed = parsers::parse_patch_feed(&body).map_err(LoreError::Parse)?;
                    let entry = self.cache.feeds.get_mut(target_list).unwrap();
                    entry.index.process_feed_page(feed);
                    entry.index.advance_offset();
                }
                Err(LoreHttpError::EndOfFeed) => {
                    if let Some(entry) = self.cache.feeds.get_mut(target_list) {
                        entry.complete = true;
                    }
                    break;
                }
                Err(e) => return Err(LoreError::Http(e)),
            }
        }

        match self.cache.feeds[target_list]
            .index
            .get_page(page_size, page_number)
        {
            Some(patches) => Ok(patches.into_iter().cloned().collect()),
            None => Err(LoreError::EndOfFeed),
        }
    }

    fn fetch_patchset_details(
        &mut self,
        representative_patch: &Patch,
        mode: CacheMode,
    ) -> Result<PatchsetDetails, LoreError> {
        let key = PatchsetCacheKey::from_patch(representative_patch);

        match mode {
            CacheMode::Refresh => {
                tracing::info!(msg_id = %key.message_id, "patchset cache: refresh requested");
                self.cache.patchsets.remove(&key);
            }
            CacheMode::UseCache => {
                if let Some(entry) = self.cache.patchsets.get(&key) {
                    if entry.is_stale(self.ttl.patchset) {
                        tracing::debug!(msg_id = %key.message_id, "patchset cache: stale, evicting");
                        self.cache.patchsets.remove(&key);
                    } else {
                        tracing::debug!(msg_id = %key.message_id, "patchset cache: hit");
                        return Ok(PatchsetDetails {
                            representative_patch: representative_patch.clone(),
                            patchset_path: entry.patchset_path.clone(),
                            raw_patches: entry.raw_patches.clone(),
                            tag_summary: entry.tag_summary.clone(),
                        });
                    }
                }
            }
            CacheMode::Bypass => {}
        }

        let patchset_path = self
            .patchset_fetcher
            .download(representative_patch)
            .map_err(|e| LoreError::PatchNotFound(e.to_string()))?;

        let raw_patches = self
            .patchset_parser
            .split_patchset(&patchset_path)
            .map_err(LoreError::Parse)?;

        let tag_summary: Vec<PatchTagSummary> =
            raw_patches.iter().map(|p| extract_tag_summary(p)).collect();

        match mode {
            CacheMode::Bypass => {}
            CacheMode::Refresh | CacheMode::UseCache => {
                self.cache.patchsets.insert(
                    key,
                    PatchsetCacheEntry::new(
                        patchset_path.clone(),
                        raw_patches.clone(),
                        tag_summary.clone(),
                    ),
                );
            }
        }

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

    fn warm_bootstrap_cache(&mut self) -> Result<BootstrapLoreData, LoreError> {
        let mailing_lists = self
            .fetch_available_lists(CacheMode::UseCache)
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "bootstrap: failed to load mailing lists");
                Vec::new()
            });
        let bookmarks = self.load_bookmarked_patchsets().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "bootstrap: failed to load bookmarks");
            Vec::new()
        });
        let reviewed = self.load_reviewed_patchsets().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "bootstrap: failed to load reviewed patchsets");
            HashMap::new()
        });
        Ok(BootstrapLoreData {
            mailing_lists,
            bookmarks,
            reviewed,
        })
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
        persistence::{MockMailingListsCacheStore, MockUserLoreStateStore},
    };
    use crate::{
        infrastructure::{file_system::MockFileSystemTrait, shell::MockShellTrait},
        lore::{
            application::cache::{
                CacheTtl, FeedCacheEntry, MailingListsCacheEntry, PatchsetCacheEntry,
                PatchsetCacheKey,
            },
            domain::{mailing_list::MailingList, patchset::PatchFeedIndex},
        },
    };

    use super::*;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn make_service(
        lists_gateway: MockListsGateway,
        feed_gateway: MockFeedGateway,
        patch_html_gateway: MockPatchHtmlGateway,
        lists_store: MockMailingListsCacheStore,
        user_state: MockUserLoreStateStore,
        fetcher: MockPatchsetFetcher,
        parser: MockPatchsetParser,
    ) -> LoreService {
        LoreService::new(
            Arc::new(lists_gateway),
            Arc::new(feed_gateway),
            Arc::new(patch_html_gateway),
            Arc::new(lists_store),
            Arc::new(user_state),
            Arc::new(fetcher),
            Arc::new(parser),
            Arc::new(MockFileSystemTrait::new()),
            Arc::new(MockShellTrait::new()),
            CacheTtl::default(),
        )
    }

    // ── mailing lists cache tests ─────────────────────────────────────────────

    #[test]
    fn fetch_available_lists_use_cache_disk_hit() {
        let mut lists_store = MockMailingListsCacheStore::new();
        lists_store
            .expect_load_available_lists()
            .times(1)
            .returning(|| Ok(vec![MailingList::new("linux-mm", "desc")]));

        let mut svc = make_service(
            MockListsGateway::new(), // gateway must NOT be called
            MockFeedGateway::new(),
            MockPatchHtmlGateway::new(),
            lists_store,
            MockUserLoreStateStore::new(),
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        let result = svc.fetch_available_lists(CacheMode::UseCache).unwrap();
        assert_eq!(1, result.len());
        assert_eq!("linux-mm", result[0].name());
    }

    #[test]
    fn fetch_available_lists_use_cache_memory_hit() {
        // Gateway and disk must NOT be called after the in-memory entry is warm.
        let lists_store = MockMailingListsCacheStore::new(); // no expectations
        let mut svc = make_service(
            MockListsGateway::new(),
            MockFeedGateway::new(),
            MockPatchHtmlGateway::new(),
            lists_store,
            MockUserLoreStateStore::new(),
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        svc.cache.lists = Some(MailingListsCacheEntry::new(vec![MailingList::new(
            "cached-list",
            "in memory",
        )]));

        let result = svc.fetch_available_lists(CacheMode::UseCache).unwrap();
        assert_eq!(1, result.len());
        assert_eq!("cached-list", result[0].name());
    }

    #[test]
    fn fetch_available_lists_refresh_paginates_and_sorts() {
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

        let mut lists_store = MockMailingListsCacheStore::new();
        lists_store
            .expect_save_available_lists()
            .times(1)
            .returning(|_| Ok(()));

        let mut svc = make_service(
            lists_gateway,
            MockFeedGateway::new(),
            MockPatchHtmlGateway::new(),
            lists_store,
            MockUserLoreStateStore::new(),
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        let lists = svc.fetch_available_lists(CacheMode::Refresh).unwrap();
        assert_eq!(320, lists.len());
        assert_eq!("accel-config", lists[0].name());
        assert_eq!("yocto-toaster", lists[319].name());
    }

    #[test]
    fn fetch_available_lists_refresh_bypasses_memory_cache() {
        // Even with a warm memory cache, Refresh must call the gateway.
        let mut lists_gateway = MockListsGateway::new();
        lists_gateway
            .expect_fetch_available_lists_page()
            .times(1)
            .returning(|_| Ok(String::new())); // empty page → break loop

        let mut lists_store = MockMailingListsCacheStore::new();
        lists_store
            .expect_save_available_lists()
            .times(1)
            .returning(|_| Ok(()));

        let mut svc = make_service(
            lists_gateway,
            MockFeedGateway::new(),
            MockPatchHtmlGateway::new(),
            lists_store,
            MockUserLoreStateStore::new(),
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        // Pre-populate the in-memory cache.
        svc.cache.lists = Some(MailingListsCacheEntry::new(vec![MailingList::new(
            "stale-list",
            "",
        )]));

        let result = svc.fetch_available_lists(CacheMode::Refresh).unwrap();
        // Network returned an empty page, so result is empty.
        assert!(result.is_empty());
        // In-memory cache was updated (cleared then set to empty result).
        assert!(svc.cache.lists.is_some());
    }

    #[test]
    fn warm_bootstrap_cache_loads_all_data() {
        let mut lists_store = MockMailingListsCacheStore::new();
        lists_store
            .expect_load_available_lists()
            .times(1)
            .returning(|| Ok(vec![MailingList::new("linux-mm", "")]));

        let mut user_state = MockUserLoreStateStore::new();
        user_state
            .expect_load_bookmarked_patchsets()
            .times(1)
            .returning(|| Ok(vec![]));
        user_state
            .expect_load_reviewed_patchsets()
            .times(1)
            .returning(|| Ok(HashMap::new()));

        let mut svc = make_service(
            MockListsGateway::new(),
            MockFeedGateway::new(),
            MockPatchHtmlGateway::new(),
            lists_store,
            user_state,
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        let data = svc.warm_bootstrap_cache().unwrap();
        assert_eq!(1, data.mailing_lists.len());
        assert_eq!("linux-mm", data.mailing_lists[0].name());
        assert!(data.bookmarks.is_empty());
        assert!(data.reviewed.is_empty());
    }

    // ── feed tests ────────────────────────────────────────────────────────────

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
            MockMailingListsCacheStore::new(),
            MockUserLoreStateStore::new(),
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        let patches = svc
            .fetch_next_patch_page(target, 1, 1, CacheMode::UseCache)
            .unwrap();
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
            MockMailingListsCacheStore::new(),
            MockUserLoreStateStore::new(),
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        let result = svc.fetch_next_patch_page("some-list", 1, 1, CacheMode::UseCache);
        assert!(matches!(result, Err(LoreError::EndOfFeed)));
    }

    #[test]
    fn fetch_next_patch_page_use_cache_hit() {
        // Pre-populate the feed cache with 1 patch; the gateway must NOT be called.
        let src = "test_samples/lore_session/process_representative_patch/patch_feed_sample_1.xml";
        let target = "some-list";

        let mut svc = make_service(
            MockListsGateway::new(),
            MockFeedGateway::new(), // no expectations
            MockPatchHtmlGateway::new(),
            MockMailingListsCacheStore::new(),
            MockUserLoreStateStore::new(),
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        let feed = {
            use crate::lore::infrastructure::parsers::parse_patch_feed;
            let xml = fs::read_to_string(src).unwrap();
            parse_patch_feed(&xml).unwrap()
        };
        let mut index = PatchFeedIndex::new(target.to_string());
        index.process_feed_page(feed);
        svc.cache
            .feeds
            .insert(target.to_string(), FeedCacheEntry::new(index));

        let patches = svc
            .fetch_next_patch_page(target, 1, 1, CacheMode::UseCache)
            .unwrap();
        assert_eq!(1, patches.len());
    }

    #[test]
    fn fetch_next_patch_page_refresh_bypasses_cache() {
        // Even with a warm cache, Refresh must hit the gateway.
        let src = "test_samples/lore_session/process_representative_patch/patch_feed_sample_1.xml";
        let target = "some-list";

        let mut feed_gateway = MockFeedGateway::new();
        feed_gateway
            .expect_fetch_patch_feed_page()
            .times(1)
            .returning(move |_, _| Ok(fs::read_to_string(src).unwrap()));

        let mut svc = make_service(
            MockListsGateway::new(),
            feed_gateway,
            MockPatchHtmlGateway::new(),
            MockMailingListsCacheStore::new(),
            MockUserLoreStateStore::new(),
            MockPatchsetFetcher::new(),
            MockPatchsetParser::new(),
        );

        // Pre-populate the cache.
        let feed = {
            let xml = fs::read_to_string(src).unwrap();
            crate::lore::infrastructure::parsers::parse_patch_feed(&xml).unwrap()
        };
        let mut index = PatchFeedIndex::new(target.to_string());
        index.process_feed_page(feed);
        svc.cache
            .feeds
            .insert(target.to_string(), FeedCacheEntry::new(index));

        let patches = svc
            .fetch_next_patch_page(target, 1, 1, CacheMode::Refresh)
            .unwrap();
        assert_eq!(1, patches.len());
    }

    // ── patchset details cache tests ──────────────────────────────────────────

    fn make_patch_for_cache(msg_id: &str) -> Patch {
        serde_json::from_value(serde_json::json!({
            "title": "test",
            "author": { "name": "T", "email": "t@t.com" },
            "link": { "@href": msg_id },
            "updated": "2023-01-01"
        }))
        .unwrap()
    }

    #[test]
    fn fetch_patchset_details_use_cache_hit() {
        // Pre-populate the cache; fetcher and parser must NOT be called.
        let patch = make_patch_for_cache("msg-1");

        let mut svc = make_service(
            MockListsGateway::new(),
            MockFeedGateway::new(),
            MockPatchHtmlGateway::new(),
            MockMailingListsCacheStore::new(),
            MockUserLoreStateStore::new(),
            MockPatchsetFetcher::new(), // no expectations
            MockPatchsetParser::new(),  // no expectations
        );

        let key = PatchsetCacheKey::from_patch(&patch);
        svc.cache.patchsets.insert(
            key,
            PatchsetCacheEntry::new(
                "/tmp/cached.mbx".to_string(),
                vec!["raw patch content".to_string()],
                vec![],
            ),
        );

        let details = svc
            .fetch_patchset_details(&patch, CacheMode::UseCache)
            .unwrap();
        assert_eq!("/tmp/cached.mbx", details.patchset_path);
        assert_eq!(1, details.raw_patches.len());
    }

    #[test]
    fn fetch_patchset_details_cache_miss_downloads() {
        // No cache entry; fetcher and parser are called once each.
        let patch = make_patch_for_cache("msg-2");

        let mut fetcher = MockPatchsetFetcher::new();
        fetcher
            .expect_download()
            .times(1)
            .returning(|_| Ok("/tmp/new.mbx".to_string()));

        let mut parser = MockPatchsetParser::new();
        parser
            .expect_split_patchset()
            .times(1)
            .returning(|_| Ok(vec!["raw".to_string()]));

        let mut svc = make_service(
            MockListsGateway::new(),
            MockFeedGateway::new(),
            MockPatchHtmlGateway::new(),
            MockMailingListsCacheStore::new(),
            MockUserLoreStateStore::new(),
            fetcher,
            parser,
        );

        let details = svc
            .fetch_patchset_details(&patch, CacheMode::UseCache)
            .unwrap();
        assert_eq!("/tmp/new.mbx", details.patchset_path);
        // The result should now be in cache.
        let key = PatchsetCacheKey::from_patch(&patch);
        assert!(svc.cache.patchsets.contains_key(&key));
    }

    #[test]
    fn fetch_patchset_details_refresh_re_downloads() {
        // Even with a warm cache, Refresh must call fetcher and parser.
        let patch = make_patch_for_cache("msg-3");

        let mut fetcher = MockPatchsetFetcher::new();
        fetcher
            .expect_download()
            .times(1)
            .returning(|_| Ok("/tmp/refreshed.mbx".to_string()));

        let mut parser = MockPatchsetParser::new();
        parser
            .expect_split_patchset()
            .times(1)
            .returning(|_| Ok(vec!["fresh raw".to_string()]));

        let mut svc = make_service(
            MockListsGateway::new(),
            MockFeedGateway::new(),
            MockPatchHtmlGateway::new(),
            MockMailingListsCacheStore::new(),
            MockUserLoreStateStore::new(),
            fetcher,
            parser,
        );

        // Pre-populate the cache.
        let key = PatchsetCacheKey::from_patch(&patch);
        svc.cache.patchsets.insert(
            key.clone(),
            PatchsetCacheEntry::new("/tmp/old.mbx".to_string(), vec![], vec![]),
        );

        let details = svc
            .fetch_patchset_details(&patch, CacheMode::Refresh)
            .unwrap();
        assert_eq!("/tmp/refreshed.mbx", details.patchset_path);
        // Cache is updated with the fresh entry.
        assert_eq!(
            "/tmp/refreshed.mbx",
            svc.cache.patchsets[&key].patchset_path
        );
    }
}
