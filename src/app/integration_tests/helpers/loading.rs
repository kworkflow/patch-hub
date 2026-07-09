use color_eyre::Result;

use crate::app::loading::LoadingIndicator;

#[derive(Default)]
pub(crate) struct FakeLoadingIndicator {
    pub(crate) starts: Vec<String>,
    pub(crate) stop_count: usize,
}

impl LoadingIndicator for FakeLoadingIndicator {
    fn start(&mut self, title: String) {
        self.starts.push(title);
    }

    fn stop(&mut self) -> Result<()> {
        self.stop_count += 1;
        Ok(())
    }
}
