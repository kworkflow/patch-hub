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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    fn make_patch(title: &str) -> Patch {
        let json = format!(
            r#"{{
                "title": "{title}",
                "author": {{ "name": "name", "email": "teste@teste.com" }},
                "link": {{ "@href": "http://lore.kernel.org/some-list/1234-1-teste@teste.com" }},
                "thr:in-reply-to": null,
                "updated": "2024-07-06T19:15:48Z"
            }}"#
        );
        serde_json::from_str::<Patch>(&json).unwrap()
    }

    fn mock_bookmarked_patchsets() -> BookmarkedPatchsets {
        BookmarkedPatchsets {
            bookmarked_patchsets: vec![
                make_patch("Patch 1"),
                make_patch("Patch 2"),
                make_patch("Patch 3"),
            ],
            patchset_index: 0,
        }
    }

    #[test]
    fn test_select_below_patchset_increments_index() {
        let mut s = mock_bookmarked_patchsets();
        let patchset_index_before = s.patchset_index;
        s.select_below_patchset();
        assert_eq!(s.patchset_index, patchset_index_before + 1);
    }

    #[test]
    fn test_select_below_patchset_stops_at_end() {
        let mut s = mock_bookmarked_patchsets();
        s.patchset_index = s.bookmarked_patchsets.len() - 1;
        s.select_below_patchset();
        assert_eq!(s.patchset_index, s.bookmarked_patchsets.len() - 1);
    }

    #[test]
    fn test_select_above_patchset_decrements_index() {
        let mut s = mock_bookmarked_patchsets();
        s.patchset_index = 2;
        s.select_above_patchset();
        assert_eq!(s.patchset_index, 1);
    }

    #[test]
    fn test_select_above_patchset_stops_at_start() {
        let mut s = mock_bookmarked_patchsets();
        s.patchset_index = 0;
        s.select_above_patchset();
        assert_eq!(s.patchset_index, 0);
    }

    #[test]
    fn test_get_selected_patchset_returns_correct_patch() {
        let s = mock_bookmarked_patchsets();
        let patch = s.get_selected_patchset();
        assert_eq!(patch.title(), "Patch 1");
    }

    #[test]
    fn test_bookmark_selected_patch_adds_new_patch() {
        let mut s = mock_bookmarked_patchsets();
        let new_patch = make_patch("New Patch");
        s.bookmark_selected_patch(&new_patch);
        assert!(s.bookmarked_patchsets.contains(&new_patch));
    }

    #[test]
    fn test_bookmark_selected_patch_does_not_duplicate() {
        let mut s = mock_bookmarked_patchsets();
        let existing_patch = make_patch("Patch 1");
        s.bookmark_selected_patch(&existing_patch);
        let count = s
            .bookmarked_patchsets
            .iter()
            .filter(|p| **p == existing_patch)
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_unbookmark_selected_patch_removes_patch() {
        let mut s = mock_bookmarked_patchsets();
        let to_remove = s.bookmarked_patchsets[1].clone();
        s.unbookmark_selected_patch(&to_remove);
        assert!(!s.bookmarked_patchsets.contains(&to_remove));
    }

    #[test]
    fn test_unbookmark_selected_patch_ignores_missing_patch() {
        let mut s = mock_bookmarked_patchsets();
        let missing = make_patch("Inexistent");
        s.unbookmark_selected_patch(&missing);
        assert_eq!(s.bookmarked_patchsets.len(), 3);
    }
}
