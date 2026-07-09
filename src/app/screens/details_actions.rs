use std::collections::{HashMap, HashSet};

use ansi_to_tui::IntoText;

use crate::{
    lore::{
        application::dto::PatchsetDetails,
        domain::patch::{Author, Patch},
    },
    render::RenderedPatchsetPreview,
};

use super::CurrentScreen;

#[derive(Clone)]
pub struct PatchsetDetailsState {
    pub representative_patch: Patch,
    /// Raw patches as plain text files
    pub raw_patches: Vec<String>,
    /// ANSI-rendered text for each patch entry, converted to ratatui `Text` by
    /// the ViewModel projection rather than stored as a UI type.
    pub patches_preview: Vec<String>,
    /// Indicates if patchset has a cover letter
    pub has_cover_letter: bool,
    /// Which patches to reply
    pub patches_to_reply: Vec<bool>,
    /// Path to the .mbx file used by `git am` when applying the patchset.
    pub patchset_path: String,
    pub preview_index: usize,
    pub preview_scroll_offset: usize,
    /// Horizontal offset
    pub preview_pan: usize,
    /// If true, display the preview in full screen
    pub preview_fullscreen: bool,
    pub patchset_actions: HashMap<PatchsetAction, bool>,
    /// For each patch, a set of `Authors` that appear in `Reviewed-by` trailers
    pub reviewed_by: Vec<HashSet<Author>>,
    /// For each patch, a set of `Authors` that appear in `Tested-by` trailers
    pub tested_by: Vec<HashSet<Author>>,
    /// For each patch, a set of `Authors` that appear in `Acked-by` trailers
    pub acked_by: Vec<HashSet<Author>>,
    pub last_screen: CurrentScreen,
}

const LAST_LINE_PADDING: usize = 10;

fn rendered_preview_height(preview: &str) -> usize {
    preview.into_text().unwrap_or_default().height()
}

#[derive(Clone, Hash, Eq, PartialEq)]
pub enum PatchsetAction {
    Bookmark,
    ReplyWithReviewedBy,
    Apply,
}

impl PatchsetDetailsState {
    pub fn from_rendered_preview(
        representative_patch: Patch,
        details: PatchsetDetails,
        rendered_preview: RenderedPatchsetPreview,
        is_patchset_bookmarked: bool,
        last_screen: CurrentScreen,
    ) -> Self {
        let mut patches_preview: Vec<String> = Vec::new();
        let mut reviewed_by: Vec<HashSet<Author>> = Vec::new();
        let mut tested_by: Vec<HashSet<Author>> = Vec::new();
        let mut acked_by: Vec<HashSet<Author>> = Vec::new();

        for (entry, tag_summary) in rendered_preview
            .entries
            .iter()
            .zip(details.tag_summary.iter())
        {
            reviewed_by.push(tag_summary.reviewed_by.clone());
            tested_by.push(tag_summary.tested_by.clone());
            acked_by.push(tag_summary.acked_by.clone());
            patches_preview.push(entry.rendered_text.clone());
        }

        let has_cover_letter = representative_patch.number_in_series() == 0;
        let patches_to_reply = vec![false; details.raw_patches.len()];

        Self {
            representative_patch,
            raw_patches: details.raw_patches,
            patchset_path: details.patchset_path,
            patches_preview,
            patches_to_reply,
            has_cover_letter,
            preview_index: 0,
            preview_scroll_offset: 0,
            preview_pan: 0,
            preview_fullscreen: false,
            patchset_actions: HashMap::from([
                (PatchsetAction::Bookmark, is_patchset_bookmarked),
                (PatchsetAction::ReplyWithReviewedBy, false),
                (PatchsetAction::Apply, false),
            ]),
            reviewed_by,
            tested_by,
            acked_by,
            last_screen,
        }
    }

    pub fn preview_next_patch(&mut self) {
        if (self.preview_index + 1) < self.patches_preview.len() {
            self.preview_index += 1;
            self.preview_scroll_offset = 0;
            self.preview_pan = 0;
        }
    }

    pub fn preview_previous_patch(&mut self) {
        if self.preview_index > 0 {
            self.preview_index -= 1;
            self.preview_scroll_offset = 0;
            self.preview_pan = 0;
        }
    }

    /// Scroll `n` lines down
    pub fn preview_scroll_down(&mut self, n: usize) {
        let number_of_lines = rendered_preview_height(&self.patches_preview[self.preview_index]);
        if (self.preview_scroll_offset + n) <= number_of_lines {
            self.preview_scroll_offset += n;
        }
    }

    /// Scroll `n` lines up
    pub fn preview_scroll_up(&mut self, n: usize) {
        self.preview_scroll_offset = self.preview_scroll_offset.saturating_sub(n);
    }

    /// Scroll to the last line
    pub fn go_to_last_line(&mut self) {
        let number_of_lines = rendered_preview_height(&self.patches_preview[self.preview_index]);
        self.preview_scroll_offset = number_of_lines.saturating_sub(LAST_LINE_PADDING);
    }

    /// Scroll to first line
    pub fn go_to_first_line(&mut self) {
        self.preview_scroll_offset = 0;
    }

    /// Move preview horizontally one column to the right
    pub fn preview_pan_right(&mut self) {
        if self.preview_pan <= 200 {
            self.preview_pan += 1;
        }
    }

    /// Move preview horizontally one column to the left
    pub fn preview_pan_left(&mut self) {
        if self.preview_pan > 0 {
            self.preview_pan -= 1;
        }
    }

    /// Move preview horizontally to start of line
    pub fn go_to_beg_of_line(&mut self) {
        self.preview_pan = 0;
    }

    /// Toggle the preview fullscreen
    pub fn toggle_preview_fullscreen(&mut self) {
        self.preview_fullscreen = !self.preview_fullscreen;
    }

    pub fn toggle_bookmark_action(&mut self) {
        self.toggle_action(PatchsetAction::Bookmark);
    }

    pub fn toggle_reply_with_reviewed_by_action(&mut self, all: bool) {
        if all {
            if self.patches_to_reply.contains(&false) {
                // If there is at least one patch not to be replied, set all to be
                self.patches_to_reply = vec![true; self.patches_to_reply.len()];
            } else {
                // If all patches are set to be replied, set none to be
                self.patches_to_reply = vec![false; self.patches_to_reply.len()];
            }
        } else if let Some(entry) = self.patches_to_reply.get_mut(self.preview_index) {
            *entry = !*entry;
        }

        if self.patches_to_reply.contains(&true) {
            self.patchset_actions
                .insert(PatchsetAction::ReplyWithReviewedBy, true);
        } else {
            self.patchset_actions
                .insert(PatchsetAction::ReplyWithReviewedBy, false);
        }
    }

    pub fn toggle_apply_action(&mut self) {
        self.toggle_action(PatchsetAction::Apply);
    }

    pub fn reset_reply_with_reviewed_by_action(&mut self) {
        self.patches_to_reply = vec![false; self.patches_to_reply.len()];
        self.patchset_actions
            .insert(PatchsetAction::ReplyWithReviewedBy, false);
    }

    pub fn toggle_action(&mut self, patchset_action: PatchsetAction) {
        let current_value = *self
            .patchset_actions
            .get(&patchset_action)
            .expect("PatchsetDetailsState::patchset_actions must be initialized properly");
        self.patchset_actions
            .insert(patchset_action, !current_value);
    }

    pub fn actions_require_user_io(&self) -> bool {
        self.patches_to_reply.contains(&true)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use serde_xml_rs::from_str;

    use super::*;

    fn test_patch() -> Patch {
        from_str(
            r#"
            <entry xmlns:thr="http://purl.org/syndication/thread/1.0">
                <author>
                    <name>Foo Bar</name>
                    <email>foo@bar.foo.bar</email>
                </author>
                <title>[PATCH 1/1] test patch</title>
                <updated>2024-07-06T19:15:48Z</updated>
                <link href="http://lore.kernel.org/some-list/1234-1-foo@bar.foo.bar" />
                <id>urn:uuid:123-abcd-1f2a3b</id>
                <content></content>
            </entry>
        "#,
        )
        .expect("test patch XML should deserialize")
    }

    fn details_state_with_preview(preview: &str) -> PatchsetDetailsState {
        PatchsetDetailsState {
            representative_patch: test_patch(),
            raw_patches: vec!["raw patch".to_string()],
            patches_preview: vec![preview.to_string()],
            has_cover_letter: false,
            patches_to_reply: vec![false],
            patchset_path: "/tmp/patchset.mbx".to_string(),
            preview_index: 0,
            preview_scroll_offset: 0,
            preview_pan: 0,
            preview_fullscreen: false,
            patchset_actions: HashMap::from([
                (PatchsetAction::Bookmark, false),
                (PatchsetAction::ReplyWithReviewedBy, false),
                (PatchsetAction::Apply, false),
            ]),
            reviewed_by: vec![HashSet::new()],
            tested_by: vec![HashSet::new()],
            acked_by: vec![HashSet::new()],
            last_screen: CurrentScreen::LatestPatchsets,
        }
    }

    #[test]
    fn rendered_height_accounts_for_rendered_text_projection() {
        let preview = "\u{1b}[32mrendered line\u{1b}[0m\nsecond line";

        assert_eq!(2, rendered_preview_height(preview));
    }

    #[test]
    fn preview_scroll_down_uses_rendered_height() {
        let mut state = details_state_with_preview("\u{1b}[32mrendered line\u{1b}[0m\nsecond line");

        state.preview_scroll_down(2);

        assert_eq!(2, state.preview_scroll_offset);
    }

    #[test]
    fn go_to_last_line_saturates_for_short_rendered_preview() {
        let mut state = details_state_with_preview("short preview");

        state.go_to_last_line();

        assert_eq!(0, state.preview_scroll_offset);
    }
}
