//! ANSI color helpers with TTY detection.
//!
//! Color is emitted only when stdout is a terminal *and* the `NO_COLOR`
//! environment variable is unset (per <https://no-color.org/>). Output
//! redirected to a file or pipe stays plain.
//!
//! Kept tiny and dependency-free; we only need a handful of styles.

use std::io::IsTerminal;
use std::sync::OnceLock;

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        std::io::stdout().is_terminal()
    })
}

fn paint(code: &str, s: &str) -> String {
    if enabled() {
        format!("\x1b[{}m{}\x1b[0m", code, s)
    } else {
        s.to_string()
    }
}

pub fn bold(s: &str) -> String {
    paint("1", s)
}
pub fn dim(s: &str) -> String {
    paint("2", s)
}
pub fn red(s: &str) -> String {
    paint("31", s)
}
pub fn green(s: &str) -> String {
    paint("32", s)
}
pub fn cyan(s: &str) -> String {
    paint("36", s)
}

/// `[ ok ]` / `[fail]` / `[warn]` prefixes used by `verify` and `init`.
pub fn ok_tag() -> String {
    bold(&green("[ ok ]"))
}
pub fn fail_tag() -> String {
    bold(&red("[fail]"))
}
pub fn info_tag() -> String {
    bold(&cyan("[info]"))
}
