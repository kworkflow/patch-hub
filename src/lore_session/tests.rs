use super::*;

#[test]
fn can_initialize_fresh_lore_session() {
    let lore_session: LoreSession = LoreSession::new("some-list".to_string());

    assert!(lore_session.get_representative_patches_ids().len() == 0,
        "`LoreSession` should initialize with an empty vector of representative patches IDs"
    );
}
