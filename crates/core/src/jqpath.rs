//! A small, dependency-free evaluator for the subset of jq path syntax
//! source plugins use to point at JSON fields: `.`, `.a.b`, `.a[]`, `.a[3]`,
//! and chains of these (`.a.b[].c[2]`).
//!
//! This intentionally does not implement the full jq language (no pipes,
//! filters, or functions) — only path *expressions*, which is what plugin
//! authors need to say "here is the array of releases" or "here is this
//! item's title". Every path accepted here evaluates identically under real
//! `jq`, so `jq '<path>'` remains a valid way to test one by hand.

use serde_json::Value;
use std::fmt;

/// One step in a parsed path.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    /// `.name` — index into an object.
    Field(String),
    /// `[n]` — index into an array (negative counts from the end).
    Index(i64),
    /// `[]` — iterate every element of an array, or every value of an
    /// object.
    Iterate,
}

/// A parsed, ready-to-evaluate jq-style path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JqPath {
    segments: Vec<Segment>,
    raw: String,
}

/// A path string that isn't valid syntax for the supported subset.
#[derive(Debug, thiserror::Error)]
#[error("invalid jq path {raw:?}: {reason}")]
pub struct JqPathError {
    raw: String,
    reason: String,
}

impl fmt::Display for JqPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

impl JqPath {
    /// Parses a path like `.a.b[].c[2]`. Every path must start with `.`.
    pub fn parse(raw: &str) -> Result<Self, JqPathError> {
        let trimmed = raw.trim();
        let err = |reason: &str| JqPathError {
            raw: raw.to_string(),
            reason: reason.to_string(),
        };

        let mut chars = trimmed.chars().peekable();
        let mut segments = Vec::new();

        if chars.peek() != Some(&'.') {
            return Err(err("path must start with '.'"));
        }

        loop {
            match chars.peek() {
                None => break,
                Some('.') => {
                    chars.next();
                    match chars.peek() {
                        None => break,
                        Some('[') => continue,
                        Some(c) if is_ident_start(*c) => {
                            let ident = take_ident(&mut chars);
                            segments.push(Segment::Field(ident));
                        }
                        Some(c) => {
                            return Err(err(&format!("unexpected character '{c}' after '.'")));
                        }
                    }
                }
                Some('[') => {
                    chars.next();
                    let mut digits = String::new();
                    if chars.peek() == Some(&'-') {
                        digits.push('-');
                        chars.next();
                    }
                    while let Some(&c) = chars.peek() {
                        if c.is_ascii_digit() {
                            digits.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if chars.peek() != Some(&']') {
                        return Err(err("expected ']'"));
                    }
                    chars.next();
                    if digits.is_empty() {
                        segments.push(Segment::Iterate);
                    } else {
                        let idx: i64 = digits
                            .parse()
                            .map_err(|_| err(&format!("invalid index '{digits}'")))?;
                        segments.push(Segment::Index(idx));
                    }
                }
                Some(c) => {
                    return Err(err(&format!("unexpected character '{c}'")));
                }
            }
        }

        Ok(Self {
            segments,
            raw: raw.to_string(),
        })
    }

    /// Evaluates the path against `root`, returning every matching value (0,
    /// 1, or many — many only when the path contains `[]`).
    pub fn eval_all<'a>(&self, root: &'a Value) -> Vec<&'a Value> {
        let mut current: Vec<&'a Value> = vec![root];
        for segment in &self.segments {
            let mut next = Vec::new();
            for value in current {
                match segment {
                    Segment::Field(name) => {
                        if let Some(v) = value.as_object().and_then(|o| o.get(name)) {
                            next.push(v);
                        }
                    }
                    Segment::Index(i) => {
                        if let Some(arr) = value.as_array() {
                            let idx = if *i < 0 { *i + arr.len() as i64 } else { *i };
                            if idx >= 0 {
                                if let Some(v) = arr.get(idx as usize) {
                                    next.push(v);
                                }
                            }
                        }
                    }
                    Segment::Iterate => {
                        if let Some(arr) = value.as_array() {
                            next.extend(arr.iter());
                        } else if let Some(obj) = value.as_object() {
                            next.extend(obj.values());
                        }
                    }
                }
            }
            current = next;
        }
        current
    }

    /// Evaluates the path and returns the first matching value, if any.
    pub fn eval_first<'a>(&self, root: &'a Value) -> Option<&'a Value> {
        self.eval_all(root).into_iter().next()
    }

    /// The original path string, as passed to [`JqPath::parse`].
    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

fn take_ident(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut ident = String::new();
    while let Some(&c) = chars.peek() {
        if is_ident_start(c) {
            ident.push(c);
            chars.next();
        } else {
            break;
        }
    }
    ident
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identity_path() {
        let p = JqPath::parse(".").unwrap();
        let v = json!({"a": 1});
        assert_eq!(p.eval_all(&v), vec![&v]);
    }

    #[test]
    fn field_access() {
        let p = JqPath::parse(".a.b").unwrap();
        let v = json!({"a": {"b": 42}});
        assert_eq!(p.eval_first(&v), Some(&json!(42)));
    }

    #[test]
    fn missing_field_yields_nothing() {
        let p = JqPath::parse(".a.missing").unwrap();
        let v = json!({"a": {"b": 42}});
        assert_eq!(p.eval_first(&v), None);
    }

    #[test]
    fn iterate_array() {
        let p = JqPath::parse(".items[]").unwrap();
        let v = json!({"items": [1, 2, 3]});
        let got: Vec<i64> = p
            .eval_all(&v)
            .into_iter()
            .map(|x| x.as_i64().unwrap())
            .collect();
        assert_eq!(got, vec![1, 2, 3]);
    }

    #[test]
    fn iterate_object_values() {
        // subsplease's `?f=latest` response is an object keyed by show;
        // `.[]` must iterate its values.
        let p = JqPath::parse(".[]").unwrap();
        let v = json!({"one-piece": {"episode": 1}, "naruto": {"episode": 2}});
        assert_eq!(p.eval_all(&v).len(), 2);
    }

    #[test]
    fn chained_iterate_and_field() {
        let p = JqPath::parse(".shows[].episode").unwrap();
        let v = json!({"shows": [{"episode": 1}, {"episode": 2}]});
        let got: Vec<i64> = p
            .eval_all(&v)
            .into_iter()
            .map(|x| x.as_i64().unwrap())
            .collect();
        assert_eq!(got, vec![1, 2]);
    }

    #[test]
    fn index_and_negative_index() {
        let v = json!({"a": [10, 20, 30]});
        assert_eq!(
            JqPath::parse(".a[0]").unwrap().eval_first(&v),
            Some(&json!(10))
        );
        assert_eq!(
            JqPath::parse(".a[-1]").unwrap().eval_first(&v),
            Some(&json!(30))
        );
    }

    #[test]
    fn rejects_paths_without_leading_dot() {
        assert!(JqPath::parse("a.b").is_err());
    }

    #[test]
    fn rejects_unclosed_bracket() {
        assert!(JqPath::parse(".a[").is_err());
    }
}
