//! The colour scheme a reader chose.
//!
//! `Auto` follows the device. `Light` and `Dark` override it. The choice lives
//! in a cookie, so the server writes it into the markup and the first paint is
//! already correct. No script is needed and nothing flashes.

use crate::error::Error;

/// How a page chooses its colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// Follow the device preference.
    #[default]
    Auto,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

impl Theme {
    /// Every theme, in the order the menu shows them.
    pub const ALL: [Self; 3] = [Self::Auto, Self::Light, Self::Dark];

    /// Reads a cookie value. Anything unrecognised means `Auto`.
    #[must_use]
    pub fn from_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::Auto,
        }
    }

    /// Reads a configuration value. An unknown word is an error, not a guess.
    ///
    /// # Errors
    ///
    /// Fails when the value is not `auto`, `light` or `dark`.
    pub fn parse(value: &str) -> Result<Self, Error> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" | "system" => Ok(Self::Auto),
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            other => Err(Error::InvalidConfig {
                reason: format!("`{other}` is not a theme; use `auto`, `light` or `dark`"),
            }),
        }
    }

    /// The value stored in the cookie and in the `data-theme` attribute.
    #[must_use]
    pub fn value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// The word the menu shows.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    /// The theme one step along the cycle. Used by the `t` keyboard shortcut.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Auto => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::Auto,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_round_trips_through_its_value() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_value(theme.value()), theme);
            assert_eq!(Theme::parse(theme.value()).expect("a known theme"), theme);
        }
    }

    #[test]
    fn an_unknown_configuration_value_is_reported() {
        assert!(Theme::parse("purple").is_err());
        assert_eq!(
            Theme::from_value("PURPLE"),
            Theme::Auto,
            "a cookie is lenient"
        );
    }

    #[test]
    fn the_cycle_returns_to_its_start() {
        assert_eq!(Theme::Auto.next(), Theme::Light);
        assert_eq!(Theme::Light.next(), Theme::Dark);
        assert_eq!(Theme::Dark.next(), Theme::Auto);
    }
}
