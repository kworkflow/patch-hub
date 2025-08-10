use crate::{infrastructure::terminal::Tui, viewmodels::ViewModel, views::View};
use std::collections::HashMap;

use color_eyre::eyre::Result;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use crate::model::Model;

#[allow(dead_code)]
pub struct App {
    model: Model,
    current_view: View,
    viewmodels: HashMap<View, Box<dyn ViewModel>>,
    terminal: Tui,
}

impl App {
    #[allow(dead_code)]
    pub fn new(model: Model, terminal: Tui) -> color_eyre::Result<Self> {
        Ok(App {
            model,
            current_view: View::MailingLists,
            viewmodels: HashMap::new(),
            terminal,
        })
    }

    #[allow(dead_code)]
    pub fn get_current_view(&self) -> View {
        self.current_view
    }

    #[allow(dead_code)]
    pub fn get_current_viewmodel(&mut self) -> &mut Box<dyn ViewModel> {
        self.viewmodels
            .entry(self.current_view)
            .or_insert_with(|| match self.current_view {
                View::MailingLists => {
                    todo!()
                }
                View::BookmarkedPatchsets => {
                    todo!()
                }
                View::LatestPatchsets => {
                    todo!()
                }
                View::PatchsetDetails => {
                    todo!()
                }
                View::EditConfig => {
                    todo!()
                }
            })
    }

    #[allow(dead_code, unreachable_code, unused_variables)]
    pub fn run(&mut self) -> Result<()> {
        loop {
            let state = &self.get_current_viewmodel().state();
            let current_view = self.get_current_view();
            current_view.render_screen(&mut self.terminal, state)?;

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Release {
                    continue;
                }

                self.get_current_viewmodel().handle_key(key);
            }
        }
    }
}
