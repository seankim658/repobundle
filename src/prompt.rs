use std::io::{BufRead, Write};

use anyhow::{Context, Result};

/// Ask a yes-or-no question and return whether the answer was yes. Anything but `y` or `yes`,
/// including no answer at all, means no.
pub fn confirm(question: &str, mut input: impl BufRead, mut output: impl Write) -> Result<bool> {
    write!(output, "{question} [y/N] ").context("failed to write the prompt")?;
    output.flush().context("failed to write the prompt")?;
    let mut answer = String::new();
    input
        .read_line(&mut answer)
        .context("failed to read the answer")?;
    Ok(is_yes(&answer))
}

fn is_yes(answer: &str) -> bool {
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn answer(text: &str) -> bool {
        confirm("Delete?", Cursor::new(text), Vec::new()).unwrap()
    }

    #[test]
    fn accepts_y_or_yes_in_any_case() {
        for text in ["y\n", "Y\n", "yes\n", " YES \n"] {
            assert!(answer(text), "{text:?}");
        }
    }

    #[test]
    fn treats_everything_else_as_no() {
        for text in ["n\n", "\n", "", "nope\n", "yes please\n"] {
            assert!(!answer(text), "{text:?}");
        }
    }

    #[test]
    fn writes_the_question_with_the_default() {
        let mut output = Vec::new();
        confirm("Delete?", Cursor::new("n\n"), &mut output).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "Delete? [y/N] ");
    }
}
