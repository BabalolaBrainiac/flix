use crossterm::event::KeyCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddInputAction {
    Changed,
    Submit,
    Cancel,
    Switch,
    None,
}

pub fn apply_add_key(buffer: &mut String, code: KeyCode) -> AddInputAction {
    match code {
        KeyCode::Esc => AddInputAction::Cancel,
        KeyCode::Tab => AddInputAction::Switch,
        KeyCode::Enter => AddInputAction::Submit,
        KeyCode::Backspace => {
            buffer.pop();
            AddInputAction::Changed
        }
        KeyCode::Char(character) => {
            buffer.push(character);
            AddInputAction::Changed
        }
        _ => AddInputAction::None,
    }
}
