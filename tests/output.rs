mod common;

use common::Fixture;

const ESCAPE: char = '\x1b';

#[test]
fn output_is_plain_when_stdout_is_not_a_terminal() {
    let fixture = Fixture::with_commit();
    let stdout = fixture.succeed(&[]);

    assert!(!stdout.contains(ESCAPE), "{stdout:?}");
}

#[test]
fn forced_color_styles_the_output() {
    let fixture = Fixture::with_commit();
    let stdout = fixture.succeed_with_color(&[]);

    assert!(stdout.contains(ESCAPE), "{stdout:?}");
    assert!(stdout.contains("Created"), "{stdout:?}");
}
