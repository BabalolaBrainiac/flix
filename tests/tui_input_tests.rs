use crossterm::event::KeyCode;
use flix::tui::input::{apply_add_key, AddInputAction};

#[test]
fn add_input_keeps_global_shortcut_characters() {
    let mut input = String::new();
    for character in "magnet:q1234".chars() {
        assert_eq!(
            apply_add_key(&mut input, KeyCode::Char(character)),
            AddInputAction::Changed
        );
    }
    assert_eq!(input, "magnet:q1234");
}

#[test]
fn add_input_returns_explicit_actions() {
    let mut input = String::from("value");
    assert_eq!(
        apply_add_key(&mut input, KeyCode::Enter),
        AddInputAction::Submit
    );
    assert_eq!(
        apply_add_key(&mut input, KeyCode::Tab),
        AddInputAction::Switch
    );
    assert_eq!(
        apply_add_key(&mut input, KeyCode::Esc),
        AddInputAction::Cancel
    );
}
