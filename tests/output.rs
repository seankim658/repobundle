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
fn plain_messages_start_with_a_badge() {
    let fixture = Fixture::with_commit();
    fixture.write_file("notes.txt", "draft");
    let stdout = fixture.succeed(&[]);
    let stderr = fixture.succeed_with_stderr(&["--force"]);

    assert!(stdout.starts_with("[✓] Created bundles/"), "{stdout:?}");
    assert!(stderr.starts_with("[!] Uncommitted changes"), "{stderr:?}");
}

#[test]
fn spinner_stays_off_a_stderr_that_is_not_a_terminal() {
    let fixture = Fixture::with_commit();
    let stderr = fixture.succeed_with_stderr(&["-o", "../snapshot.bundle"]);

    assert!(stderr.is_empty(), "{stderr:?}");
}

#[test]
fn forced_color_styles_the_output() {
    let fixture = Fixture::with_commit();
    let stdout = fixture.succeed_with_color(&[]);

    assert!(stdout.contains(ESCAPE), "{stdout:?}");
    assert!(stdout.contains("Created"), "{stdout:?}");
}
