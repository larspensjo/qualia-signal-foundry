//! ANSI styling helpers. Emits escape codes only when output is a TTY and
//! the user has not disabled color via NO_COLOR or an explicit no-color flag.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

static NO_COLOR_FLAG: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorMode {
    Enabled,
    Disabled,
}

impl ColorMode {
    pub fn detect(stdout_is_tty: bool, no_color_env: Option<&str>, no_color_flag: bool) -> Self {
        let no_color_env_set = no_color_env.is_some_and(|value| !value.is_empty());
        if no_color_flag || no_color_env_set || !stdout_is_tty {
            Self::Disabled
        } else {
            Self::Enabled
        }
    }

    pub fn for_stdout() -> Self {
        let stdout_is_tty = std::io::stdout().is_terminal();
        let no_color_env = std::env::var("NO_COLOR").ok();

        Self::detect(
            stdout_is_tty,
            no_color_env.as_deref(),
            NO_COLOR_FLAG.load(Ordering::Relaxed),
        )
    }

    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

pub fn set_no_color_flag(disabled: bool) {
    NO_COLOR_FLAG.store(disabled, Ordering::Relaxed);
}

#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub fg_256: Option<u8>,
    pub dim: bool,
    pub italic: bool,
}

impl Style {
    pub const fn fg(code: u8) -> Self {
        Self {
            fg_256: Some(code),
            dim: false,
            italic: false,
        }
    }

    pub const fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
}

pub const STYLE_DIRECT_HEADER: Style = Style::fg(44).dim();
pub const STYLE_DIRECT_BODY: Style = Style::fg(51);
pub const STYLE_HINT_BLOCK: Style = Style::fg(214).dim();
pub const STYLE_DROP_MARKER: Style = Style::fg(240).dim().italic();

pub fn paint(mode: ColorMode, style: Style, text: &str) -> String {
    if !mode.is_enabled() {
        return text.to_string();
    }

    let mut prefix = String::new();
    if style.dim {
        prefix.push_str("\x1b[2m");
    }
    if style.italic {
        prefix.push_str("\x1b[3m");
    }
    if let Some(code) = style.fg_256 {
        prefix.push_str(&format!("\x1b[38;5;{code}m"));
    }
    if prefix.is_empty() {
        return text.to_string();
    }

    format!("{prefix}{text}\x1b[0m")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_disabled_when_no_color_env_has_non_empty_value() {
        assert_eq!(
            ColorMode::detect(true, Some("1"), false),
            ColorMode::Disabled
        );
    }

    #[test]
    fn detect_enabled_when_no_color_env_is_empty() {
        assert_eq!(ColorMode::detect(true, Some(""), false), ColorMode::Enabled);
    }

    #[test]
    fn detect_disabled_when_flag_set() {
        assert_eq!(ColorMode::detect(true, None, true), ColorMode::Disabled);
    }

    #[test]
    fn detect_disabled_when_not_tty() {
        assert_eq!(ColorMode::detect(false, None, false), ColorMode::Disabled);
    }

    #[test]
    fn detect_enabled_when_tty_and_no_flags() {
        assert_eq!(ColorMode::detect(true, None, false), ColorMode::Enabled);
    }

    #[test]
    fn paint_passes_through_when_disabled() {
        assert_eq!(
            paint(ColorMode::Disabled, STYLE_DIRECT_HEADER, "hello"),
            "hello"
        );
    }

    #[test]
    fn paint_emits_escape_codes_when_enabled() {
        let out = paint(ColorMode::Enabled, STYLE_DIRECT_HEADER, "hello");

        assert!(out.starts_with("\x1b[2m") || out.starts_with("\x1b["));
        assert!(out.contains("hello"));
        assert!(out.ends_with("\x1b[0m"));
    }
}
