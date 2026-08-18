use flix::cli_playback::parse_subtitle_selection;
use flix::cli_search::{parse_choice, Choice};

#[test]
fn an_empty_answer_selects_the_first_entry() {
    assert_eq!(parse_choice("", 5, true), Some(Choice::Selected(0)));
    assert_eq!(parse_choice("  ", 5, false), Some(Choice::Selected(0)));
}

#[test]
fn the_search_prompt_reads_back_and_quit() {
    assert_eq!(parse_choice("b", 5, true), Some(Choice::Back));
    assert_eq!(parse_choice("BACK", 5, true), Some(Choice::Back));
    assert_eq!(parse_choice("q", 5, true), Some(Choice::Quit));
    assert_eq!(parse_choice("quit", 5, false), Some(Choice::Quit));
}

#[test]
fn the_first_search_step_has_no_back() {
    assert_eq!(parse_choice("b", 5, false), None);
}

#[test]
fn the_search_prompt_rejects_a_number_out_of_range() {
    assert_eq!(parse_choice("3", 5, true), Some(Choice::Selected(2)));
    assert_eq!(parse_choice("5", 5, true), Some(Choice::Selected(4)));
    assert_eq!(parse_choice("6", 5, true), None);
    assert_eq!(parse_choice("0", 5, true), None);
    assert_eq!(parse_choice("x", 5, true), None);
}

#[test]
fn the_subtitle_prompt_reads_one_or_several_numbers() {
    assert_eq!(parse_subtitle_selection("", 4), Some(vec![0]));
    assert_eq!(parse_subtitle_selection("2", 4), Some(vec![1]));
    assert_eq!(parse_subtitle_selection("1,3", 4), Some(vec![0, 2]));
    assert_eq!(parse_subtitle_selection("1 3", 4), Some(vec![0, 2]));
}

#[test]
fn the_subtitle_prompt_drops_a_repeated_number() {
    assert_eq!(parse_subtitle_selection("2,2,1", 4), Some(vec![1, 0]));
}

#[test]
fn the_subtitle_prompt_reads_none_and_top() {
    assert_eq!(parse_subtitle_selection("0", 4), Some(Vec::new()));
    assert_eq!(parse_subtitle_selection("none", 4), Some(Vec::new()));
    assert_eq!(parse_subtitle_selection("a", 4), Some(vec![0, 1, 2]));
    // The top choice never asks for more subtitles than the search found.
    assert_eq!(parse_subtitle_selection("all", 2), Some(vec![0, 1]));
}

#[test]
fn the_subtitle_prompt_rejects_an_invalid_answer() {
    assert_eq!(parse_subtitle_selection("5", 4), None);
    assert_eq!(parse_subtitle_selection("1,9", 4), None);
    assert_eq!(parse_subtitle_selection("x", 4), None);
}

#[test]
fn no_subtitle_result_needs_no_answer() {
    assert_eq!(parse_subtitle_selection("1", 0), Some(Vec::new()));
}
