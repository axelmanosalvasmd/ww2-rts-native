//! Inactive proposal: complete JSON structure with exact UTF16 keys and values.
//! Not registered in Cargo, a server callback, a client decoder or the private view.
use super::exact_text::{ExactText, JsonStringErrorKind, decode_json_string};
use serde_json::{Number, Value};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExactValue {
    Null,
    Bool(bool),
    Number(Number),
    Text(ExactText),
    Array(Vec<ExactValue>),
    Object(Vec<(ExactText, ExactValue)>),
}
impl ExactValue {
    pub(crate) fn get(&self, key: &str) -> Option<&Self> {
        let Self::Object(fields) = self else { return None; };
        fields.iter().find(|(name, _)| name.units().iter().copied().eq(key.encode_utf16()))
            .map(|(_, value)| value)
    }
    pub(crate) fn text(&self) -> Option<&ExactText> {
        if let Self::Text(text) = self { Some(text) } else { None }
    }
    // Scalar conversion is explicit and fallible at every key and value.
    // This must not be used to erase lone units from an adopted exact owner.
    pub(crate) fn scalar_value(&self) -> Result<Value, std::string::FromUtf16Error> {
        Ok(match self {
            Self::Null => Value::Null,
            Self::Bool(value) => Value::Bool(*value),
            Self::Number(value) => Value::Number(value.clone()),
            Self::Text(value) => Value::String(value.scalar_string()?),
            Self::Array(values) => Value::Array(values.iter().map(Self::scalar_value)
                .collect::<Result<Vec<_>, _>>()?),
            Self::Object(fields) => {
                let mut object = serde_json::Map::new();
                for (key, value) in fields {
                    object.insert(key.scalar_string()?, value.scalar_value()?);
                }
                Value::Object(object)
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParseErrorKind {
    ExpectedValue,
    ExpectedKey,
    ExpectedColon,
    ExpectedSeparator,
    InvalidLiteral,
    InvalidNumber,
    String(JsonStringErrorKind),
    Depth,
    TrailingInput,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParseError {
    pub(crate) offset: usize,
    pub(crate) kind: ParseErrorKind,
}

// The caller owns frame-byte and container-depth policy. No wire limit changes here.
pub(crate) fn parse(input: &str, max_container_depth: usize) -> Result<ExactValue, ParseError> {
    let mut parser = Parser { input, at: 0 };
    let value = parser.value(max_container_depth)?;
    parser.whitespace();
    if parser.at != input.len() { return Err(parser.error(ParseErrorKind::TrailingInput)); }
    Ok(value)
}
struct Parser<'a> {
    input: &'a str,
    at: usize,
}
impl Parser<'_> {
    fn error(&self, kind: ParseErrorKind) -> ParseError { ParseError { offset: self.at, kind } }
    fn peek(&self) -> Option<u8> { self.input.as_bytes().get(self.at).copied() }
    fn take(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) { self.at += 1; true } else { false }
    }
    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) { self.at += 1; }
    }
    fn text(&mut self) -> Result<ExactText, ParseError> {
        let (text, consumed) = decode_json_string(&self.input[self.at..])
            .map_err(|error| ParseError {
                offset: self.at + error.offset,
                kind: ParseErrorKind::String(error.kind),
            })?;
        self.at += consumed;
        Ok(text)
    }
    fn literal(&mut self, spelling: &str, value: ExactValue) -> Result<ExactValue, ParseError> {
        if !self.input[self.at..].starts_with(spelling) {
            return Err(self.error(ParseErrorKind::InvalidLiteral));
        }
        self.at += spelling.len();
        Ok(value)
    }
    fn number(&mut self) -> Result<ExactValue, ParseError> {
        let begin = self.at;
        while matches!(self.peek(), Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')) {
            self.at += 1;
        }
        let value = serde_json::from_str::<Number>(&self.input[begin..self.at])
            .map_err(|_| ParseError { offset: begin, kind: ParseErrorKind::InvalidNumber })?;
        Ok(ExactValue::Number(value))
    }
    fn value(&mut self, depth: usize) -> Result<ExactValue, ParseError> {
        self.whitespace();
        match self.peek() {
            Some(b'n') => self.literal("null", ExactValue::Null),
            Some(b't') => self.literal("true", ExactValue::Bool(true)),
            Some(b'f') => self.literal("false", ExactValue::Bool(false)),
            Some(b'"') => self.text().map(ExactValue::Text),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b'[') if depth > 0 => self.array(depth - 1),
            Some(b'{') if depth > 0 => self.object(depth - 1),
            Some(b'[' | b'{') => Err(self.error(ParseErrorKind::Depth)),
            _ => Err(self.error(ParseErrorKind::ExpectedValue)),
        }
    }
    fn array(&mut self, depth: usize) -> Result<ExactValue, ParseError> {
        self.at += 1;
        self.whitespace();
        let mut values = Vec::new();
        if self.take(b']') { return Ok(ExactValue::Array(values)); }
        loop {
            values.push(self.value(depth)?);
            self.whitespace();
            if self.take(b']') { return Ok(ExactValue::Array(values)); }
            if !self.take(b',') { return Err(self.error(ParseErrorKind::ExpectedSeparator)); }
        }
    }
    fn object(&mut self, depth: usize) -> Result<ExactValue, ParseError> {
        self.at += 1;
        self.whitespace();
        let mut fields: Vec<(ExactText, ExactValue)> = Vec::new();
        if self.take(b'}') { return Ok(ExactValue::Object(fields)); }
        loop {
            self.whitespace();
            if self.peek() != Some(b'"') { return Err(self.error(ParseErrorKind::ExpectedKey)); }
            let key = self.text()?;
            self.whitespace();
            if !self.take(b':') { return Err(self.error(ParseErrorKind::ExpectedColon)); }
            let value = self.value(depth)?;
            if let Some((_, previous)) = fields.iter_mut().find(|entry| entry.0.units() == key.units()) {
                *previous = value;
            } else {
                fields.push((key, value));
            }
            self.whitespace();
            if self.take(b'}') { return Ok(ExactValue::Object(fields)); }
            if !self.take(b',') { return Err(self.error(ParseErrorKind::ExpectedSeparator)); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn complete_message_retains_exact_named_and_unknown_string_units() {
        let value = parse(r#"{"\u0074":"hello","token":"\ud800","name":"\ud801","listing":{"title":"\ud83d"},"unknown":["\udc00"]}"#, 3).unwrap();
        assert_eq!(value.get("t").unwrap().text().unwrap().scalar_string().unwrap(), "hello");
        assert_eq!(value.get("token").unwrap().text().unwrap().units(), &[0xd800]);
        assert_eq!(value.get("name").unwrap().text().unwrap().units(), &[0xd801]);
        assert_eq!(value.get("listing").unwrap().get("title").unwrap().text().unwrap().units(), &[0xd83d]);
        assert_eq!(value.get("unknown"), Some(&ExactValue::Array(vec![ExactValue::Text(ExactText::from_units([0xdc00]))])));
        assert!(value.scalar_value().is_err());
    }

    #[test]
    fn duplicate_keys_replace_by_exact_units_without_collapsing_lone_keys() {
        let value = parse(r#"{"name":"first","\u006eame":"second","\ud800":1,"\ud801":2}"#, 1).unwrap();
        let ExactValue::Object(fields) = &value else { panic!("object required"); };
        assert_eq!(fields.len(), 3);
        assert_eq!(value.get("name").unwrap().text().unwrap().scalar_string().unwrap(), "second");
        assert_eq!(fields[1].0.units(), &[0xd800]);
        assert_eq!(fields[2].0.units(), &[0xd801]);
        assert!(value.scalar_value().is_err());
    }

    #[test]
    fn scalar_messages_keep_existing_json_types_and_escape_boundaries() {
        let input = r#"{"name":"\\\"[]{}:,","keep":[null,true,false,-7,1.25,1e2],"\u0074":"hello"}"#;
        let value = parse(input, 2).unwrap().scalar_value().unwrap();
        assert_eq!(value, serde_json::from_str::<Value>(input).unwrap());
        assert_eq!(value["keep"], json!([null,true,false,-7,1.25,100.0]));
        assert_eq!(value["t"], "hello");
    }

    #[test]
    fn complete_parser_rejects_trailing_malformed_and_unseparated_values() {
        for input in ["", "true false", "[1,]", "{\"a\":1,}", "{\"a\" 1}", "[01]", "[1e]", "[+1]", "{\"name\":\"\\ud800\" trailing}"] {
            assert!(parse(input, 8).is_err(), "{input:?}");
        }
        assert_eq!(parse("true false", 1).unwrap_err().kind, ParseErrorKind::TrailingInput);
    }

    #[test]
    fn caller_depth_budget_counts_containers_without_changing_string_units() {
        assert!(parse(r#""\ud800""#, 0).is_ok());
        assert!(parse(r#"{"name":"Fern"}"#, 1).is_ok());
        assert_eq!(parse(r#"{"listing":{"title":"\ud800"}}"#, 1).unwrap_err().kind, ParseErrorKind::Depth);
        assert!(parse(r#"{"listing":{"title":"\ud800"}}"#, 2).is_ok());
    }
}
