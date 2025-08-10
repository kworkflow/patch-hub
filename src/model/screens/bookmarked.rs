use crate::lore::patch::Patch;

pub struct BookmarkedPatchsets {
    pub bookmarked_patchsets: Vec<Patch>,
    pub patchset_index: usize,
}

impl BookmarkedPatchsets {
    pub fn select_below_patchset(&mut self) {
        if self.patchset_index + 1 < self.bookmarked_patchsets.len() {
            self.patchset_index += 1;
        }
    }

    pub fn select_above_patchset(&mut self) {
        self.patchset_index = self.patchset_index.saturating_sub(1);
    }

    pub fn get_selected_patchset(&self) -> Patch {
        self.bookmarked_patchsets
            .get(self.patchset_index)
            .unwrap()
            .clone()
    }

    pub fn bookmark_selected_patch(&mut self, patch_to_bookmark: &Patch) {
        if !self.bookmarked_patchsets.contains(patch_to_bookmark) {
            self.bookmarked_patchsets.push(patch_to_bookmark.clone());
        }
    }

    pub fn unbookmark_selected_patch(&mut self, patch_to_unbookmark: &Patch) {
        if let Some(index) = self
            .bookmarked_patchsets
            .iter()
            .position(|r| r == patch_to_unbookmark)
        {
            self.bookmarked_patchsets.remove(index);
        }
    }
}
