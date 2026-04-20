use crate::lore::patch::Patch;

pub struct BookmarkedPatchsets {
    /// List of all bookmarked patchsets
    pub bookmarked_patchsets: Vec<Patch>,
    /// Index of the currently selected patchset
    pub patchset_index: usize,
}

impl BookmarkedPatchsets {
    /// Increments `patchset_index` by one, unless the last patchset
    /// is already selected.
    pub fn select_below_patchset(&mut self) {
        if self.patchset_index + 1 < self.bookmarked_patchsets.len() {
            self.patchset_index += 1;
        }
    }

    /// Decrements `patchset_index` by one, unless the first patchset
    //  is alredy selected.
    pub fn select_above_patchset(&mut self) {
        self.patchset_index = self.patchset_index.saturating_sub(1);
    }

    /// Get a clone of the actual selected patchset
    pub fn get_selected_patchset(&self) -> Patch {
        self.bookmarked_patchsets
            .get(self.patchset_index)
            .unwrap()
            .clone()
    }

    /// Add a patchset to the list of bookmarks if is not already on the list
    pub fn bookmark_selected_patch(&mut self, patch_to_bookmark: &Patch) {
        if !self.bookmarked_patchsets.contains(patch_to_bookmark) {
            self.bookmarked_patchsets.push(patch_to_bookmark.clone());
        }
    }

    /// Remove a patchset from the list of bookmarks
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
