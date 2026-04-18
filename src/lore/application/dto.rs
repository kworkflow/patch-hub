use std::collections::HashSet;

use crate::lore::domain::patch::{Author, Patch};

/// Per-patch tag summary extracted from a raw patch's cover section.
pub struct PatchTagSummary {
    pub reviewed_by: HashSet<Author>,
    pub tested_by: HashSet<Author>,
    pub acked_by: HashSet<Author>,
}

/// Complete data for displaying and acting on a patchset in the UI.
pub struct PatchsetDetails {
    /// Kept for future actor-model phases that will need the representative patch.
    #[allow(dead_code)]
    pub representative_patch: Patch,
    /// Path on disk to the downloaded `.mbx` file (needed for `git am`).
    pub patchset_path: String,
    /// Raw text of each individual patch (cover letter first, if present).
    pub raw_patches: Vec<String>,
    /// Per-patch tag summary (same length and order as `raw_patches`).
    pub tag_summary: Vec<PatchTagSummary>,
}
