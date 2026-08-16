use flix::player::install::{confirm, verify_and_finalize};
use std::io::Cursor;

#[test]
fn confirmation_requires_an_explicit_yes() {
    assert!(confirm(&mut Cursor::new(b"yes\n")).expect("confirmation"));
    assert!(confirm(&mut Cursor::new(b"y\n")).expect("confirmation"));
    assert!(!confirm(&mut Cursor::new(b"\n")).expect("confirmation"));
    assert!(!confirm(&mut Cursor::new(b"no\n")).expect("confirmation"));
}

#[test]
fn checksum_mismatch_removes_the_partial_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let partial = directory.path().join("player.partial");
    let destination = directory.path().join("player");
    std::fs::write(&partial, b"invalid").expect("write partial");

    let result = verify_and_finalize(
        &partial,
        &destination,
        "0000000000000000000000000000000000000000000000000000000000000000",
    );

    assert!(result.is_err());
    assert!(!partial.exists());
    assert!(!destination.exists());
}

#[test]
fn valid_checksum_moves_the_partial_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let partial = directory.path().join("player.partial");
    let destination = directory.path().join("player");
    std::fs::write(&partial, b"valid").expect("write partial");

    verify_and_finalize(
        &partial,
        &destination,
        "ec654fac9599f62e79e2706abef23dfb7c07c08185aa86db4d8695f0b718d1b3",
    )
    .expect("valid checksum");

    assert!(!partial.exists());
    assert_eq!(
        std::fs::read(destination).expect("read destination"),
        b"valid"
    );
}
