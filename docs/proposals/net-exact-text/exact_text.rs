//! Review draft only. Not registered or connected to a wire/storage consumer.
//! Exact v0 text units and a shape-preserving JSON string writer.
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExactText(Box<[u16]>);

impl ExactText {
    pub(crate) fn from_units(units: impl IntoIterator<Item = u16>) -> Self {
        Self(units.into_iter().collect())
    }

    pub(crate) fn token(units: &[u16]) -> Self {
        Self::from_units(units.iter().take(40).copied())
    }

    pub(crate) fn clean(units: &[u16], limit: usize, fallback: &str, controls: bool) -> Self {
        let filtered = units.iter().copied().filter(|unit| {
            !matches!(*unit, 0x3c | 0x3e | 0x26 | 0x22 | 0x27) && !(controls && *unit < 32)
        }).collect::<Vec<_>>();
        let first = filtered.iter().position(|unit| !trim_unit(*unit)).unwrap_or(filtered.len());
        let last = filtered.iter().rposition(|unit| !trim_unit(*unit)).map_or(first, |at| at + 1);
        let end = first + limit.min(last - first);
        if first == end {
            Self::from_units(fallback.encode_utf16())
        } else {
            Self::from_units(filtered[first..end].iter().copied())
        }
    }

    pub(crate) fn units(&self) -> &[u16] {
        &self.0
    }

    // A caller requiring Unicode scalar strings must handle this error.
    // No implicit FFFD conversion or invented fallback is available here.
    pub(crate) fn scalar_string(&self) -> Result<String, std::string::FromUtf16Error> {
        String::from_utf16(&self.0)
    }

    fn write_json(&self, out: &mut String) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        out.push('"');
        for unit in self.0.iter().copied() {
            out.push_str("\\u");
            for shift in [12, 8, 4, 0] {
                out.push(char::from(HEX[usize::from((unit >> shift) & 15)]));
            }
        }
        out.push('"');
    }
}

fn trim_unit(unit: u16) -> bool {
    matches!(unit,
        0x0009..=0x000d | 0x0020 | 0x00a0 | 0x1680 | 0x2000..=0x200a |
        0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Part {
    Key(String),
    Index(usize),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TextPathError {
    MissingPath,
    NotString,
    DuplicatePath,
}

// The template owns the same existing fields/types. Replacements can target
// only present string values, and the public wire never contains this map.
pub(crate) struct TextMessage {
    base: Value,
    text: BTreeMap<Vec<Part>, ExactText>,
}

impl TextMessage {
    pub(crate) fn new(base: Value) -> Self {
        Self { base, text: BTreeMap::new() }
    }

    pub(crate) fn set(&mut self, path: Vec<Part>, text: ExactText) -> Result<(), TextPathError> {
        let mut value = &self.base;
        for part in &path {
            value = match part {
                Part::Key(key) => value.as_object().and_then(|object| object.get(key)),
                Part::Index(index) => value.as_array().and_then(|array| array.get(*index)),
            }.ok_or(TextPathError::MissingPath)?;
        }
        if !value.is_string() {
            return Err(TextPathError::NotString);
        }
        if self.text.contains_key(&path) {
            return Err(TextPathError::DuplicatePath);
        }
        self.text.insert(path, text);
        Ok(())
    }

    pub(crate) fn encode(&self) -> String {
        let mut out = String::new();
        self.write(&self.base, &mut Vec::new(), &mut out);
        out
    }

    fn write(&self, value: &Value, path: &mut Vec<Part>, out: &mut String) {
        if let Some(text) = self.text.get(path) {
            text.write_json(out);
            return;
        }
        match value {
            Value::Array(array) => {
                out.push('[');
                for (index, value) in array.iter().enumerate() {
                    if index != 0 { out.push(','); }
                    path.push(Part::Index(index));
                    self.write(value, path, out);
                    path.pop();
                }
                out.push(']');
            }
            Value::Object(object) => {
                out.push('{');
                for (index, (key, value)) in object.iter().enumerate() {
                    if index != 0 { out.push(','); }
                    out.push_str(&serde_json::to_string(key).expect("String serialization cannot fail"));
                    out.push(':');
                    path.push(Part::Key(key.clone()));
                    self.write(value, path, out);
                    path.pop();
                }
                out.push('}');
            }
            _ => out.push_str(&value.to_string()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JsonStringErrorKind {
    ExpectedQuote,
    Unterminated,
    UnescapedControl,
    InvalidEscape,
    InvalidHex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JsonStringError {
    pub(crate) offset: usize,
    pub(crate) kind: JsonStringErrorKind,
}

// Decode one JSON string token from valid UTF8, preserving every UTF16 unit.
// The caller must validate the surrounding JSON and the returned byte boundary.
// This primitive is not a full-message parser and is not registered anywhere.
pub(crate) fn decode_json_string(input: &str) -> Result<(ExactText, usize), JsonStringError> {
    let error = |offset, kind| JsonStringError { offset, kind };
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'"') {
        return Err(error(0, JsonStringErrorKind::ExpectedQuote));
    }
    let mut units = Vec::new();
    let mut at = 1_usize;
    while let Some(byte) = bytes.get(at).copied() {
        match byte {
            b'"' => return Ok((ExactText::from_units(units), at + 1)),
            b'\\' => {
                at += 1;
                let escape = bytes.get(at).copied()
                    .ok_or_else(|| error(at, JsonStringErrorKind::Unterminated))?;
                at += 1;
                if escape == b'u' {
                    let digits = bytes.get(at..at.saturating_add(4))
                        .ok_or_else(|| error(at, JsonStringErrorKind::InvalidHex))?;
                    let mut unit = 0_u16;
                    for (index, digit) in digits.iter().copied().enumerate() {
                        let hex = match digit {
                            b'0'..=b'9' => digit - b'0',
                            b'a'..=b'f' => digit - b'a' + 10,
                            b'A'..=b'F' => digit - b'A' + 10,
                            _ => return Err(error(at + index, JsonStringErrorKind::InvalidHex)),
                        };
                        unit = (unit << 4) | u16::from(hex);
                    }
                    units.push(unit);
                    at += 4;
                } else {
                    units.push(match escape {
                        b'"' => 0x22,
                        b'\\' => 0x5c,
                        b'/' => 0x2f,
                        b'b' => 0x08,
                        b'f' => 0x0c,
                        b'n' => 0x0a,
                        b'r' => 0x0d,
                        b't' => 0x09,
                        _ => return Err(error(at - 1, JsonStringErrorKind::InvalidEscape)),
                    });
                }
            }
            0..=31 => return Err(error(at, JsonStringErrorKind::UnescapedControl)),
            _ => {
                let scalar = input[at..].chars().next()
                    .ok_or_else(|| error(at, JsonStringErrorKind::Unterminated))?;
                let mut encoded = [0_u16; 2];
                units.extend_from_slice(scalar.encode_utf16(&mut encoded));
                at += scalar.len_utf8();
            }
        }
    }
    Err(error(at, JsonStringErrorKind::Unterminated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_text_keeps_split_surrogates_and_original_cleaning_boundaries() {
        let source = format!("{}\u{1f600}", "c".repeat(15)).encode_utf16().collect::<Vec<_>>();
        let text = ExactText::clean(&source, 16, "Soldier", false);
        assert_eq!(text.units().len(), 16);
        assert_eq!(text.units()[15], 0xd83d);
        assert!(text.scalar_string().is_err());
        assert!(!text.units().contains(&0xfffd));
        let units = "\u{feff}<Name>\u{feff}".encode_utf16().collect::<Vec<_>>();
        assert_eq!(ExactText::clean(&units, 16, "Soldier", false).units(), &[78,97,109,101]);
        let units = "\u{0085}Name\u{0085}".encode_utf16().collect::<Vec<_>>();
        assert_eq!(ExactText::clean(&units, 16, "Soldier", false).units(), &[133,78,97,109,101,133]);
        assert_eq!(ExactText::clean(&[0xfeff, 0x3c, 0x3e], 16, "Soldier", false).scalar_string().unwrap(), "Soldier");
        assert_eq!(ExactText::clean(&[78, 1, 97], 16, "Soldier", false).units(), &[78,1,97]);
        assert_eq!(ExactText::clean(&[78, 1, 97], 48, "Open skirmish", true).units(), &[78,97]);
    }

    #[test]
    fn exact_token_identity_opposes_scalar_slicing_and_replacement() {
        let a = format!("{}\u{10000}", "x".repeat(39)).encode_utf16().collect::<Vec<_>>();
        let b = format!("{}\u{10001}", "x".repeat(39)).encode_utf16().collect::<Vec<_>>();
        let c = format!("{}\u{10400}", "x".repeat(39)).encode_utf16().collect::<Vec<_>>();
        assert_eq!(ExactText::token(&a), ExactText::token(&b));
        assert_ne!(ExactText::token(&a), ExactText::token(&c));
        assert_eq!(ExactText::token(&a).units()[39], 0xd800);
        assert_eq!(ExactText::token(&c).units()[39], 0xd801);
        assert_eq!(ExactText::token(&[]).units(), &[]);
    }

    #[test]
    fn exact_text_wire_keeps_json_string_type_and_every_surrogate_unit() {
        let mut message = TextMessage::new(json!({"name":"", "keep":[true,7,null]}));
        message.set(vec![Part::Key("name".into())], ExactText::from_units([0xd83d, 0xde00, 0xd800, 0x22, 0x5c, 0x000a])).unwrap();
        assert_eq!(message.encode(), r#"{"keep":[true,7,null],"name":"\ud83d\ude00\ud800\u0022\u005c\u000a"}"#);
        let mut paired = TextMessage::new(json!({"name":""}));
        paired.set(vec![Part::Key("name".into())], ExactText::from_units([0xd83d,0xde00])).unwrap();
        let decoded: Value = serde_json::from_str(&paired.encode()).unwrap();
        assert_eq!(decoded, json!({"name":"\u{1f600}"}));
    }

    #[test]
    fn exact_text_paths_reject_missing_wrong_type_and_duplicate_targets() {
        let mut message = TextMessage::new(json!({"players":[{"name":""}],"count":1}));
        assert_eq!(message.set(vec![Part::Key("missing".into())], ExactText::from_units([65])), Err(TextPathError::MissingPath));
        assert_eq!(message.set(vec![Part::Key("count".into())], ExactText::from_units([65])), Err(TextPathError::NotString));
        assert_eq!(message.set(vec![Part::Key("players".into()),Part::Index(1)], ExactText::from_units([65])), Err(TextPathError::MissingPath));
        let path = vec![Part::Key("players".into()),Part::Index(0),Part::Key("name".into())];
        message.set(path.clone(), ExactText::from_units([65])).unwrap();
        assert_eq!(message.set(path, ExactText::from_units([66])), Err(TextPathError::DuplicatePath));
        assert_eq!(serde_json::from_str::<Value>(&message.encode()).unwrap(), json!({"players":[{"name":"A"}],"count":1}));
    }

    #[test]
    fn exact_json_string_preserves_literal_pairs_and_escaped_lone_units() {
        for (input, expected) in [
            (r#""😀""#, vec![0xd83d, 0xde00]),
            (r#""\uD83D\ude00""#, vec![0xd83d, 0xde00]),
            (r#""\ud800A\udc00""#, vec![0xd800, 65, 0xdc00]),
            (r#""\uD801""#, vec![0xd801]),
        ] {
            let (text, used) = decode_json_string(input).unwrap();
            assert_eq!(used, input.len());
            assert_eq!(text.units(), expected.as_slice());
            assert!(!text.units().contains(&0xfffd));
        }
        let a = decode_json_string(r#""\ud800""#).unwrap().0;
        let b = decode_json_string(r#""\ud801""#).unwrap().0;
        assert_ne!(a, b);
        assert!(a.scalar_string().is_err());
    }

    #[test]
    fn exact_json_string_roundtrip_preserves_controls_and_token_boundary() {
        let source = ExactText::from_units([0, 8, 12, 10, 13, 9, 34, 92, 47, 0xfeff, 0xd800, 0xdc00, 0xd801]);
        let mut encoded = String::new();
        source.write_json(&mut encoded);
        let (decoded, used) = decode_json_string(&encoded).unwrap();
        assert_eq!(used, encoded.len());
        assert_eq!(decoded, source);
        let input = "\"\\uD800\",false";
        let (decoded, used) = decode_json_string(input).unwrap();
        assert_eq!(decoded.units(), &[0xd800]);
        assert_eq!(&input[used..], ",false");
        let (short, _) = decode_json_string(r#""\"\\\/\b\f\n\r\t""#).unwrap();
        assert_eq!(short.units(), &[34, 92, 47, 8, 12, 10, 13, 9]);
    }

    #[test]
    fn exact_json_string_rejects_invalid_syntax_without_lossy_recovery() {
        for input in ["", "null", "\"\\x20\"", "\"\\uD80\"", "\"\\uD80G\"", "\"\n\"", "\"\\'", "\"unterminated"] {
            assert!(decode_json_string(input).is_err(), "{input:?}");
        }
        assert_eq!(decode_json_string("\"\n\"").unwrap_err(), JsonStringError {
            offset: 1, kind: JsonStringErrorKind::UnescapedControl,
        });
    }
}
