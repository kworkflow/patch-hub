mod core;
pub mod errors;
pub mod loading_screen;
mod painter;
pub mod scene;
mod screens;
pub mod theme;

use crate::app::view_model::AppViewModel;

pub fn draw_ui(f: &mut ratatui::Frame, vm: &AppViewModel) {
    let ui_core = core::UiCore::new();
    match ui_core.build_scene(vm) {
        Ok(scene) => painter::paint(f, &scene),
        Err(e) => tracing::error!("failed to build ui scene: {e}"),
    }
}
