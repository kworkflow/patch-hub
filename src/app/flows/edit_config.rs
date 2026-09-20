use color_eyre::Result;
use tracing::debug;

use crate::{
    app::{popup::AppPopup, screens::CurrentScreen, App},
    input::event::InputEvent,
};

pub async fn handle_edit_config(app: &mut App, input: InputEvent) -> Result<()> {
    let Some(is_editing) = app
        .state
        .config_state
        .edit_config
        .as_ref()
        .map(|edit_config_state| edit_config_state.is_editing())
    else {
        return Ok(());
    };

    match is_editing {
        true => {
            if let Some(edit_config_state) = app.state.config_state.edit_config.as_mut() {
                match input {
                    InputEvent::CancelConfigEdit => {
                        edit_config_state.clear_edit();
                        edit_config_state.toggle_editing();
                    }
                    InputEvent::Backspace => {
                        edit_config_state.backspace_edit();
                    }
                    InputEvent::TextInput(ch) => {
                        edit_config_state.append_edit(ch);
                    }
                    InputEvent::StageConfigEdit => {
                        edit_config_state.stage_edit();
                        edit_config_state.clear_edit();
                        edit_config_state.toggle_editing();
                    }
                    InputEvent::NavigateLeft => {
                        edit_config_state.cycle_edit(false);
                    }
                    InputEvent::NavigateRight => {
                        edit_config_state.cycle_edit(true);
                    }
                    _ => {}
                }
            }
        }
        false => match input {
            InputEvent::OpenHelp => {
                let popup = generate_help_popup();
                app.state.popup = Some(popup);
            }
            InputEvent::SaveConfig => {
                debug!("saving edited configuration");
                app.consolidate_edit_config().await?;
                app.reset_edit_config();
                app.set_current_screen(CurrentScreen::MailingListSelection);
            }
            InputEvent::EditConfigField => {
                if let Some(edit_config_state) = app.state.config_state.edit_config.as_mut() {
                    edit_config_state.toggle_editing();
                }
            }
            InputEvent::NavigateDown => {
                if let Some(edit_config_state) = app.state.config_state.edit_config.as_mut() {
                    edit_config_state.highlight_next();
                }
            }
            InputEvent::NavigateUp => {
                if let Some(edit_config_state) = app.state.config_state.edit_config.as_mut() {
                    edit_config_state.highlight_prev();
                }
            }
            _ => {}
        },
    }
    Ok(())
}

pub fn generate_help_popup() -> AppPopup {
    AppPopup::help()
        .title("Edit Config")
        .description("This screen allows you to edit the configuration options for patch-hub.\nKernel trees are added by editing the config file; this screen selects among existing keys.")
        .keybind("ESC / q", "Save and exit")
        .keybind("ENTER", "Edit the highlighted option")
        .keybind("?", "Show this help screen")
        .keybind("j/🡇", "Down")
        .keybind("k/🡅", "Up")
        .keybind("←/→", "Cycle the target kernel tree while editing that row")
        .build()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        env::VarError,
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
    };

    use tokio::sync::mpsc;

    use crate::{
        app::{
            screens::{
                bookmarked::BookmarkedPatchsetsState, mail_list::MailingListSelectionState,
                CurrentScreen,
            },
            state::{AppState, ConfigUiState, LoreUiState, NavigationState, UserLoreState},
            App, AppServices,
        },
        config::{bootstrap_parts, ConfigActor, ConfigHandle, DEFAULT_CONFIG_PATH_SUFFIX},
        infrastructure::{
            env::MockEnvTrait, file_system::MockFileSystemTrait, file_system::OsFileSystem,
            shell::MockShellTrait,
        },
        kw::history::MockKwHistoryStore,
        lore::{application::handle::LoreApiHandle, domain::mailing_list::MailingList},
        render::handle::RenderHandle,
    };

    use super::*;

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let n = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!(
            "patch-hub-edit-config-{prefix}-{}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn default_env() -> (MockEnvTrait, PathBuf) {
        let home = unique_test_dir("home");
        let home_s = home.to_string_lossy().into_owned();
        let mut mock = MockEnvTrait::new();
        mock.expect_var()
            .withf(|key| key == "PATCH_HUB_CONFIG_PATH")
            .returning(|_| Err(VarError::NotPresent.into()));
        mock.expect_var()
            .withf(move |key| key == "HOME")
            .returning(move |_| Ok(home_s.clone()));
        mock.expect_var()
            .withf(|key| {
                matches!(
                    key,
                    "PATCH_HUB_PAGE_SIZE"
                        | "PATCH_HUB_CACHE_DIR"
                        | "PATCH_HUB_DATA_DIR"
                        | "PATCH_HUB_GIT_SEND_EMAIL_OPTIONS"
                        | "PATCH_HUB_PATCH_RENDERER"
                )
            })
            .returning(|_| Err(VarError::NotPresent.into()));
        (mock, home)
    }

    fn app_with_trees() -> (App, ConfigHandle, PathBuf) {
        let (env, home) = default_env();
        let (mut state, repo) = bootstrap_parts(&env, OsFileSystem).unwrap();
        state.kernel_trees.insert(
            "linux".into(),
            serde_json::from_value(serde_json::json!({
                "path": "/linux",
                "branch": "master"
            }))
            .unwrap(),
        );
        state.kernel_trees.insert(
            "zebra".into(),
            serde_json::from_value(serde_json::json!({
                "path": "/zebra",
                "branch": "master"
            }))
            .unwrap(),
        );
        let snapshot = state.to_snapshot();
        let config = ConfigActor::spawn(state, repo);

        let dummy_list = MailingList::new("test-list", "Test list");
        let (lore_tx, _lore_rx) = mpsc::channel(1);
        let (render_tx, _render_rx) = mpsc::channel(1);
        let app = App {
            state: AppState {
                navigation: NavigationState {
                    current_screen: CurrentScreen::EditConfig,
                },
                lore: LoreUiState {
                    mailing_list_selection: MailingListSelectionState {
                        mailing_lists: vec![dummy_list.clone()],
                        target_list: String::new(),
                        possible_mailing_lists: vec![dummy_list],
                        highlighted_list_index: 0,
                    },
                    latest_patchsets: None,
                    details: None,
                },
                user_state: UserLoreState {
                    bookmarked_patchsets: BookmarkedPatchsetsState {
                        bookmarked_patchsets: vec![],
                        patchset_index: 0,
                    },
                    reviewed_patchsets: HashMap::new(),
                },
                config_state: ConfigUiState { edit_config: None },
                config: snapshot,
                popup: None,
                kw: Default::default(),
            },
            services: AppServices {
                lore_api: LoreApiHandle::new(lore_tx),
                render: RenderHandle::new(render_tx),
                shell: Box::new(MockShellTrait::new()),
                fs: Arc::new(MockFileSystemTrait::new()),
                config: config.clone(),
                kw_history: Arc::new(MockKwHistoryStore::new()),
                kw: None,
            },
        };
        (app, config, home)
    }

    #[test]
    fn help_documents_tree_cycle_and_save_keys() {
        let AppPopup::Help {
            description,
            formatted_keybinds,
            ..
        } = generate_help_popup()
        else {
            panic!("expected help popup");
        };
        assert!(description
            .as_deref()
            .is_some_and(|text| text.contains("selects among existing keys")));
        assert!(formatted_keybinds.contains("ESC / q: Save and exit"));
        assert!(formatted_keybinds.contains("ENTER: Edit the highlighted option"));
        assert!(formatted_keybinds.contains("←/→: Cycle the target kernel tree"));
        assert!(!formatted_keybinds.contains("Save changes"));
    }

    #[tokio::test]
    async fn cycling_the_tree_row_and_saving_updates_the_snapshot() {
        let (mut app, config, home) = app_with_trees();
        app.init_edit_config();

        for _ in 0..11 {
            handle_edit_config(&mut app, InputEvent::NavigateDown)
                .await
                .unwrap();
        }
        handle_edit_config(&mut app, InputEvent::EditConfigField)
            .await
            .unwrap();
        handle_edit_config(&mut app, InputEvent::NavigateRight)
            .await
            .unwrap();
        handle_edit_config(&mut app, InputEvent::StageConfigEdit)
            .await
            .unwrap();
        handle_edit_config(&mut app, InputEvent::SaveConfig)
            .await
            .unwrap();

        assert_eq!(
            Some("linux"),
            app.state.config.target_kernel_tree().as_deref()
        );
        assert!(app.state.config_state.edit_config.is_none());
        assert_eq!(
            CurrentScreen::MailingListSelection,
            app.state.navigation.current_screen
        );

        let raw = fs::read_to_string(home.join(DEFAULT_CONFIG_PATH_SUFFIX)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed["target_kernel_tree"], "linux");
        config.shutdown().await;
    }
}
