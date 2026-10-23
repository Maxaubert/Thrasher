//! Filename templates such as `clip_{n}` or `{date}_{time}_{n:2}`.
//!
//! Placeholders:
//! - `{n}` or `{n:W}`: the clip number, zero padded to width W (default 3).
//! - `{date}` or `{date:FMT}`: the start date, `%Y-%m-%d` by default.
//! - `{time}` or `{time:FMT}`: the start time, `%H-%M-%S` by default.
//!
//! `FMT` uses chrono's strftime syntax.

use chrono::NaiveDateTime;
use std::fmt::Write;
use thiserror::Error;

pub const DEFAULT_TEMPLATE: &str = "clip_{n}";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TemplateError {
    #[error("unclosed '{{' in template")]
    Unclosed,
    #[error("unknown placeholder '{0}'")]
    Unknown(String),
    #[error("bad width in '{0}'")]
    BadWidth(String),
    #[error("bad date or time format '{0}'")]
    BadFormat(String),
    #[error(
        "the template must contain {{n}}, {{time}} or {{date}} with seconds so names stay unique"
    )]
    NotUnique,
    #[error("the template makes a name with characters Windows does not allow")]
    BadChars,
    #[error("the template is empty")]
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Lit(String),
    Number(usize),
    Date(String),
    Time(String),
}

/// A parsed filename template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    tokens: Vec<Token>,
}

impl Template {
    pub fn parse(src: &str) -> Result<Self, TemplateError> {
        if src.trim().is_empty() {
            return Err(TemplateError::Empty);
        }
        let mut tokens = Vec::new();
        let mut lit = String::new();
        let mut rest = src;
        while let Some(i) = rest.find('{') {
            lit.push_str(&rest[..i]);
            let after = &rest[i + 1..];
            let end = after.find('}').ok_or(TemplateError::Unclosed)?;
            let inner = &after[..end];
            rest = &after[end + 1..];
            if !lit.is_empty() {
                tokens.push(Token::Lit(std::mem::take(&mut lit)));
            }
            let (name, arg) = match inner.split_once(':') {
                Some((n, a)) => (n, Some(a)),
                None => (inner, None),
            };
            let token = match name {
                "n" => {
                    let width = match arg {
                        Some(a) => a
                            .parse()
                            .map_err(|_| TemplateError::BadWidth(inner.into()))?,
                        None => 3,
                    };
                    Token::Number(width)
                }
                "date" => Token::Date(arg.unwrap_or("%Y-%m-%d").to_string()),
                "time" => Token::Time(arg.unwrap_or("%H-%M-%S").to_string()),
                _ => return Err(TemplateError::Unknown(inner.into())),
            };
            if let Token::Date(f) | Token::Time(f) = &token {
                check_format(f)?;
            }
            tokens.push(token);
        }
        lit.push_str(rest);
        if !lit.is_empty() {
            tokens.push(Token::Lit(lit));
        }
        let unique = tokens.iter().any(|t| match t {
            Token::Number(_) => true,
            Token::Time(f) | Token::Date(f) => f.contains("%S") || f.contains("%T"),
            Token::Lit(_) => false,
        });
        if !unique {
            return Err(TemplateError::NotUnique);
        }
        let t = Self { tokens };
        let sample = t.render(1, chrono::Local::now().naive_local());
        if sample
            .chars()
            .any(|c| "<>:\"/\\|?*".contains(c) || c.is_control())
            || sample.ends_with(['.', ' '])
        {
            return Err(TemplateError::BadChars);
        }
        Ok(t)
    }

    pub fn has_number(&self) -> bool {
        self.tokens.iter().any(|t| matches!(t, Token::Number(_)))
    }

    pub fn render(&self, n: u32, at: NaiveDateTime) -> String {
        let mut out = String::new();
        for t in &self.tokens {
            match t {
                Token::Lit(s) => out.push_str(s),
                Token::Number(w) => {
                    let _ = write!(out, "{n:0w$}", w = *w);
                }
                Token::Date(f) | Token::Time(f) => {
                    let _ = write!(out, "{}", at.format(f));
                }
            }
        }
        out
    }

    /// The clip number in `name`, if `name` could have come from this template.
    pub fn number_in(&self, name: &str) -> Option<u32> {
        if !self.has_number() {
            return None;
        }
        match_tokens(&self.tokens, name)
    }

    /// The next free name: one past the highest number among `existing`, skipping
    /// names that are taken.
    pub fn next_name<'a>(
        &self,
        existing: impl IntoIterator<Item = &'a str> + Clone,
        at: NaiveDateTime,
    ) -> String {
        let max = existing
            .clone()
            .into_iter()
            .filter_map(|e| self.number_in(e))
            .max()
            .unwrap_or(0);
        let taken: std::collections::HashSet<String> = existing
            .into_iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
        let free = |name: &str| !taken.contains(&name.to_ascii_lowercase());
        if !self.has_number() {
            let base = self.render(0, at);
            if free(&base) {
                return base;
            }
            return (2..)
                .map(|k| format!("{base}_{k}"))
                .find(|n| free(n))
                .unwrap();
        }
        (max + 1..)
            .map(|n| self.render(n, at))
            .find(|n| free(n))
            .unwrap()
    }
}

fn check_format(f: &str) -> Result<(), TemplateError> {
    use chrono::format::{Item, StrftimeItems};
    if StrftimeItems::new(f).any(|i| matches!(i, Item::Error)) {
        return Err(TemplateError::BadFormat(f.into()));
    }
    Ok(())
}

/// Backtracking match: literals must match exactly, date and time match any text,
/// the number matches one or more digits.
fn match_tokens(tokens: &[Token], s: &str) -> Option<u32> {
    fn go(tokens: &[Token], s: &str, found: Option<u32>) -> Option<Option<u32>> {
        let Some((first, rest)) = tokens.split_first() else {
            return s.is_empty().then_some(found);
        };
        match first {
            Token::Lit(l) => s.strip_prefix(l.as_str()).and_then(|r| go(rest, r, found)),
            Token::Number(_) => {
                let digits = s.bytes().take_while(u8::is_ascii_digit).count();
                (1..=digits).rev().find_map(|k| {
                    let n = s[..k].parse().ok()?;
                    go(rest, &s[k..], Some(n))
                })
            }
            Token::Date(_) | Token::Time(_) => (0..=s.len())
                .filter(|&k| s.is_char_boundary(k))
                .find_map(|k| go(rest, &s[k..], found)),
        }
    }
    go(tokens, s, None).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 23)
            .unwrap()
            .and_hms_opt(14, 5, 9)
            .unwrap()
    }

    #[test]
    fn default_matches_old_recorder() {
        let t = Template::parse(DEFAULT_TEMPLATE).unwrap();
        assert_eq!(t.render(7, at()), "clip_007");
        assert_eq!(t.number_in("clip_042"), Some(42));
        assert_eq!(t.number_in("clip_1234"), Some(1234));
        assert_eq!(t.number_in("other"), None);
    }

    #[test]
    fn date_time_and_width() {
        let t = Template::parse("{date}_{time}_{n:2}").unwrap();
        assert_eq!(t.render(3, at()), "2026-10-23_14-05-09_03");
        let t = Template::parse("{date:%d.%m.%Y} take {n:1}").unwrap();
        assert_eq!(t.render(12, at()), "23.10.2026 take 12");
        assert_eq!(t.number_in("01.01.2025 take 9"), Some(9));
    }

    #[test]
    fn next_name_continues_numbering() {
        let t = Template::parse("clip_{n}").unwrap();
        let existing = ["clip_001", "clip_005", "notes"];
        assert_eq!(t.next_name(existing, at()), "clip_006");
        let none: [&str; 0] = [];
        assert_eq!(t.next_name(none, at()), "clip_001");
    }

    #[test]
    fn next_name_without_number_avoids_collisions() {
        let t = Template::parse("{date}_{time}").unwrap();
        assert_eq!(t.next_name(["x"], at()), "2026-10-23_14-05-09");
        let existing = ["2026-10-23_14-05-09", "2026-10-23_14-05-09_2"];
        assert_eq!(t.next_name(existing, at()), "2026-10-23_14-05-09_3");
    }

    #[test]
    fn rejects_bad_templates() {
        assert_eq!(Template::parse(""), Err(TemplateError::Empty));
        assert_eq!(Template::parse("clip_{n"), Err(TemplateError::Unclosed));
        assert!(matches!(
            Template::parse("{foo}"),
            Err(TemplateError::Unknown(_))
        ));
        assert_eq!(Template::parse("recording"), Err(TemplateError::NotUnique));
        assert_eq!(Template::parse("a/{n}"), Err(TemplateError::BadChars));
        assert_eq!(
            Template::parse("{time:%H:%M:%S}"),
            Err(TemplateError::BadChars)
        );
    }
}
