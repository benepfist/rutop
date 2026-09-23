//! User/db/host filters (mytop's `StringOrRegex`).

use std::fmt;

use anyhow::{Context, Result};
use regex::Regex;

#[derive(Clone, Debug)]
pub enum Filter {
    /// Matches everything.
    Any,
    /// Exact string match.
    Exact(String),
    /// Regular expression (unanchored, like Perl's `=~`).
    Regex(Regex),
}

impl Filter {
    /// Parse interactive input: blank = everything, `/re/` = regex, else exact match.
    pub fn from_input(input: &str) -> Result<Filter> {
        let input = input.trim_end_matches(['\r', '\n']);
        if input.is_empty() {
            return Ok(Filter::Any);
        }
        if input.len() >= 2 && input.starts_with('/') && input.ends_with('/') {
            return Filter::raw_regex(&input[1..input.len() - 1]);
        }
        if input == "/" {
            return Ok(Filter::Any);
        }
        Ok(Filter::Exact(input.to_string()))
    }

    /// A bare regex as used for `filter_*` keys in `~/.mytop`.
    pub fn raw_regex(re: &str) -> Result<Filter> {
        if re.is_empty() {
            return Ok(Filter::Any);
        }
        let regex = Regex::new(re).with_context(|| format!("invalid regex /{re}/"))?;
        Ok(Filter::Regex(regex))
    }

    pub fn matches(&self, value: &str) -> bool {
        match self {
            Filter::Any => true,
            Filter::Exact(s) => s == value,
            Filter::Regex(r) => r.is_match(value),
        }
    }

    pub fn is_any(&self) -> bool {
        matches!(self, Filter::Any)
    }
}

impl fmt::Display for Filter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Filter::Any => f.write_str("(all)"),
            Filter::Exact(s) => write!(f, "{s}"),
            Filter::Regex(r) => write!(f, "/{}/", r.as_str()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Filters {
    pub user: Filter,
    pub db: Filter,
    pub host: Filter,
}

impl Filters {
    pub fn clear(&mut self) {
        self.user = Filter::Any;
        self.db = Filter::Any;
        self.host = Filter::Any;
    }

    pub fn any_active(&self) -> bool {
        !(self.user.is_any() && self.db.is_any() && self.host.is_any())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_matches_all() {
        let f = Filter::from_input("\n").unwrap();
        assert!(f.matches(""));
        assert!(f.matches("anything"));
    }

    #[test]
    fn exact_is_anchored_and_literal() {
        let f = Filter::from_input("a.b").unwrap();
        assert!(f.matches("a.b"));
        assert!(!f.matches("axb"));
        assert!(!f.matches("xa.b"));
    }

    #[test]
    fn regex_input() {
        let f = Filter::from_input("/^web[0-9]+/").unwrap();
        assert!(f.matches("web12"));
        assert!(!f.matches("db1"));
        assert!(Filter::from_input("/[/").is_err());
    }
}
