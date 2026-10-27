//! Password strength policy (docs/SPEC.md "Rules"): at least 12 characters
//! and a zxcvbn score of 3 or more.

use serde::Serialize;

pub const MIN_CHARS: usize = 12;
pub const MIN_SCORE: u8 = 3;

/// What the setup screen's strength meter shows.
#[derive(Debug, Clone, Serialize)]
pub struct Strength {
    /// zxcvbn score, 0 (trivial) to 4 (very strong).
    pub score: u8,
    pub acceptable: bool,
    /// Why it's unacceptable, or zxcvbn's warning; None if nothing to say.
    pub feedback: Option<String>,
}

pub fn strength(password: &str) -> Strength {
    let entropy = zxcvbn::zxcvbn(password, &["privenator", "journal", "diary"]);
    let score = u8::from(entropy.score());
    let warning = entropy.feedback().and_then(|f| f.warning()).map(|w| w.to_string());

    let feedback = if password.chars().count() < MIN_CHARS {
        Some(format!("Use at least {MIN_CHARS} characters."))
    } else if score < MIN_SCORE {
        Some(warning.unwrap_or_else(|| "Too easy to guess. Add more unrelated words.".into()))
    } else {
        warning
    };
    Strength { score, acceptable: password.chars().count() >= MIN_CHARS && score >= MIN_SCORE, feedback }
}
