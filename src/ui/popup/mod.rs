pub mod help;
pub mod info_popup;
pub mod review_trailers;

use ratatui::{layout::Rect, Frame};

use std::fmt::Debug;

use crate::input::event::InputEvent;

pub trait PopUpClone {
    fn clone_box(&self) -> Box<dyn PopUp>;
}

impl<T> PopUpClone for T
where
    T: 'static + PopUp + Clone,
{
    fn clone_box(&self) -> Box<dyn PopUp> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn PopUp> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// A trait that represents a popup that can be rendered on top of a screen
pub trait PopUp: Debug + Send + PopUpClone {
    /// Returns the dimensions of the popup in percentage of the screen
    /// (width, height)
    ///
    /// Those dimensions are used to create the `chunk` used in the render function
    fn dimensions(&self) -> (u16, u16);

    /// Renders the popup on the given frame using the given chunk
    /// This chunk is a centered rectangle with the dimensions returned by `dimensions`
    fn render(&self, f: &mut Frame, chunk: Rect);

    /// Handles semantic input for the popup.
    ///
    /// Is important to notice that except for close events, all other keys are hijacked by the popup
    /// So the screens handlers won't be called
    fn handle(&mut self, input: InputEvent) -> color_eyre::Result<()>;
}
