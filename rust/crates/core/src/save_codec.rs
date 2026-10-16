//! A JSON envelope for the full Serde value model used by match saves.
//! Numbers and map keys are tagged so JSON cannot round or coerce them.

use serde::Serialize;
use serde::de::{self, DeserializeOwned, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use serde::ser::{
    self, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use serde_json::{Value, json};

#[derive(Debug)]
struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl ser::Error for Error {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}
impl de::Error for Error {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

#[derive(Debug)]
enum Node {
    Bool(bool),
    I(i64),
    U(u64),
    I128(i128),
    U128(u128),
    F32(u32),
    F64(u64),
    Char(char),
    String(String),
    Bytes(Vec<u8>),
    Unit,
    None,
    Some(Box<Node>),
    Newtype(Box<Node>),
    Seq(Vec<Node>),
    Map(Vec<(Node, Node)>),
}

impl Node {
    fn json(self) -> Value {
        match self {
            Self::Bool(v) => json!(["bool", v]),
            Self::I(v) => json!(["i", v]),
            Self::U(v) => json!(["u", v]),
            Self::I128(v) => json!(["i128", v.to_string()]),
            Self::U128(v) => json!(["u128", v.to_string()]),
            Self::F32(v) => json!(["f32", v]),
            Self::F64(v) => json!(["f64", v]),
            Self::Char(v) => json!(["char", v]),
            Self::String(v) => json!(["string", v]),
            Self::Bytes(v) => json!(["bytes", v]),
            Self::Unit => json!(["unit"]),
            Self::None => json!(["none"]),
            Self::Some(v) => json!(["some", v.json()]),
            Self::Newtype(v) => json!(["newtype", v.json()]),
            Self::Seq(v) => json!(["seq", v.into_iter().map(Node::json).collect::<Vec<_>>()]),
            Self::Map(v) => json!([
                "map",
                v.into_iter()
                    .map(|(k, v)| [k.json(), v.json()])
                    .collect::<Vec<_>>()
            ]),
        }
    }

    fn from_json(value: Value) -> Result<Self, Error> {
        let Value::Array(mut a) = value else {
            return Err(Error("expected tagged array".into()));
        };
        if a.is_empty() {
            return Err(Error("empty tagged array".into()));
        }
        let tag = a
            .remove(0)
            .as_str()
            .ok_or_else(|| Error("invalid tag".into()))?
            .to_owned();
        let wanted = if tag == "unit" || tag == "none" { 0 } else { 1 };
        if a.len() != wanted {
            return Err(Error(format!("invalid {tag} length")));
        }
        let v = a.pop().unwrap_or(Value::Null);
        fn parse<T: DeserializeOwned>(v: Value) -> Result<T, Error> {
            serde_json::from_value(v).map_err(|e| Error(e.to_string()))
        }
        Ok(match tag.as_str() {
            "bool" => Self::Bool(parse(v)?),
            "i" => Self::I(parse(v)?),
            "u" => Self::U(parse(v)?),
            "i128" => Self::I128(
                parse::<String>(v)?
                    .parse()
                    .map_err(|e: std::num::ParseIntError| Error(e.to_string()))?,
            ),
            "u128" => Self::U128(
                parse::<String>(v)?
                    .parse()
                    .map_err(|e: std::num::ParseIntError| Error(e.to_string()))?,
            ),
            "f32" => Self::F32(parse(v)?),
            "f64" => Self::F64(parse(v)?),
            "char" => Self::Char(parse(v)?),
            "string" => Self::String(parse(v)?),
            "bytes" => Self::Bytes(parse(v)?),
            "unit" => Self::Unit,
            "none" => Self::None,
            "some" => Self::Some(Box::new(Self::from_json(v)?)),
            "newtype" => Self::Newtype(Box::new(Self::from_json(v)?)),
            "seq" => Self::Seq(
                parse::<Vec<Value>>(v)?
                    .into_iter()
                    .map(Self::from_json)
                    .collect::<Result<_, _>>()?,
            ),
            "map" => Self::Map(
                parse::<Vec<[Value; 2]>>(v)?
                    .into_iter()
                    .map(|[k, v]| Ok((Self::from_json(k)?, Self::from_json(v)?)))
                    .collect::<Result<_, Error>>()?,
            ),
            _ => return Err(Error(format!("unknown tag {tag}"))),
        })
    }
}

pub(crate) fn to_value<T: Serialize>(value: &T) -> Result<Value, String> {
    value
        .serialize(Encoder)
        .map(Node::json)
        .map_err(|e: Error| e.0)
}

pub(crate) fn from_value<T: DeserializeOwned>(value: Value) -> Result<T, String> {
    T::deserialize(Node::from_json(value).map_err(|e| e.0)?).map_err(|e: Error| e.0)
}

/// Replace process addresses in routing identities with stable save IDs.
/// Unknown addresses represent detached caches and become zero, which forces
/// a rebuild on the next route query after load.
pub(crate) fn normalize_pointer_fields(
    mut value: Value,
    bindings: &std::collections::BTreeMap<usize, usize>,
) -> Value {
    fn field_name(node: &Value) -> Option<&str> {
        let a = node.as_array()?;
        (a.len() == 2 && a[0] == "string")
            .then(|| a[1].as_str())
            .flatten()
    }
    fn normalize_number(node: &mut Value, bindings: &std::collections::BTreeMap<usize, usize>) {
        let Some(a) = node.as_array_mut() else { return };
        if a.len() == 2 && a[0] == "some" {
            normalize_number(&mut a[1], bindings);
        } else if a.len() == 2
            && a[0] == "u"
            && let Some(old) = a[1].as_u64().and_then(|n| usize::try_from(n).ok())
        {
            a[1] = Value::from(bindings.get(&old).copied().unwrap_or(0) as u64);
        }
    }
    fn visit(node: &mut Value, bindings: &std::collections::BTreeMap<usize, usize>) {
        let Some(a) = node.as_array_mut() else { return };
        if a.len() != 2 {
            return;
        }
        if a[0] == "map" {
            let Some(entries) = a[1].as_array_mut() else {
                return;
            };
            let identity = ["width", "height", "knowledge_id"].iter().all(|wanted| {
                entries.iter().any(|entry| {
                    entry.as_array().is_some_and(|pair| {
                        pair.len() == 2 && field_name(&pair[0]) == Some(*wanted)
                    })
                })
            });
            for entry in entries {
                let Some(pair) = entry.as_array_mut() else {
                    continue;
                };
                if pair.len() != 2 {
                    continue;
                }
                let name = field_name(&pair[0]);
                if name == Some("initial_ptr")
                    || (identity && matches!(name, Some("flags" | "heights")))
                {
                    normalize_number(&mut pair[1], bindings);
                }
                visit(&mut pair[1], bindings);
            }
        } else if a[0] == "seq" {
            if let Some(children) = a[1].as_array_mut() {
                for child in children {
                    visit(child, bindings);
                }
            }
        } else if a[0] == "some" || a[0] == "newtype" {
            visit(&mut a[1], bindings);
        }
    }
    visit(&mut value, bindings);
    value
}

struct Encoder;
impl ser::Serializer for Encoder {
    type Ok = Node;
    type Error = Error;
    type SerializeSeq = Sequence;
    type SerializeTuple = Sequence;
    type SerializeTupleStruct = Sequence;
    type SerializeTupleVariant = VariantSequence;
    type SerializeMap = Mapping;
    type SerializeStruct = Mapping;
    type SerializeStructVariant = VariantMapping;
    fn serialize_bool(self, v: bool) -> Result<Node, Error> {
        Ok(Node::Bool(v))
    }
    fn serialize_i8(self, v: i8) -> Result<Node, Error> {
        Ok(Node::I(v.into()))
    }
    fn serialize_i16(self, v: i16) -> Result<Node, Error> {
        Ok(Node::I(v.into()))
    }
    fn serialize_i32(self, v: i32) -> Result<Node, Error> {
        Ok(Node::I(v.into()))
    }
    fn serialize_i64(self, v: i64) -> Result<Node, Error> {
        Ok(Node::I(v))
    }
    fn serialize_i128(self, v: i128) -> Result<Node, Error> {
        Ok(Node::I128(v))
    }
    fn serialize_u8(self, v: u8) -> Result<Node, Error> {
        Ok(Node::U(v.into()))
    }
    fn serialize_u16(self, v: u16) -> Result<Node, Error> {
        Ok(Node::U(v.into()))
    }
    fn serialize_u32(self, v: u32) -> Result<Node, Error> {
        Ok(Node::U(v.into()))
    }
    fn serialize_u64(self, v: u64) -> Result<Node, Error> {
        Ok(Node::U(v))
    }
    fn serialize_u128(self, v: u128) -> Result<Node, Error> {
        Ok(Node::U128(v))
    }
    fn serialize_f32(self, v: f32) -> Result<Node, Error> {
        Ok(Node::F32(v.to_bits()))
    }
    fn serialize_f64(self, v: f64) -> Result<Node, Error> {
        Ok(Node::F64(v.to_bits()))
    }
    fn serialize_char(self, v: char) -> Result<Node, Error> {
        Ok(Node::Char(v))
    }
    fn serialize_str(self, v: &str) -> Result<Node, Error> {
        Ok(Node::String(v.into()))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Node, Error> {
        Ok(Node::Bytes(v.into()))
    }
    fn serialize_none(self) -> Result<Node, Error> {
        Ok(Node::None)
    }
    fn serialize_some<T: ?Sized + Serialize>(self, v: &T) -> Result<Node, Error> {
        Ok(Node::Some(Box::new(v.serialize(self)?)))
    }
    fn serialize_unit(self) -> Result<Node, Error> {
        Ok(Node::Unit)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Node, Error> {
        Ok(Node::Unit)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
    ) -> Result<Node, Error> {
        Ok(Node::String(name.into()))
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<Node, Error> {
        Ok(Node::Newtype(Box::new(v.serialize(self)?)))
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        v: &T,
    ) -> Result<Node, Error> {
        Ok(Node::Map(vec![(
            Node::String(name.into()),
            v.serialize(self)?,
        )]))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Sequence, Error> {
        Ok(Sequence(Vec::new()))
    }
    fn serialize_tuple(self, _: usize) -> Result<Sequence, Error> {
        Ok(Sequence(Vec::new()))
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Sequence, Error> {
        Ok(Sequence(Vec::new()))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        _: usize,
    ) -> Result<VariantSequence, Error> {
        Ok(VariantSequence(name, Vec::new()))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Mapping, Error> {
        Ok(Mapping(Vec::new(), None))
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Mapping, Error> {
        Ok(Mapping(Vec::new(), None))
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        _: usize,
    ) -> Result<VariantMapping, Error> {
        Ok(VariantMapping(name, Vec::new()))
    }
    fn collect_str<T: ?Sized + std::fmt::Display>(self, v: &T) -> Result<Node, Error> {
        Ok(Node::String(v.to_string()))
    }
}

struct Sequence(Vec<Node>);
impl SerializeSeq for Sequence {
    type Ok = Node;
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Error> {
        self.0.push(v.serialize(Encoder)?);
        Ok(())
    }
    fn end(self) -> Result<Node, Error> {
        Ok(Node::Seq(self.0))
    }
}
impl SerializeTuple for Sequence {
    type Ok = Node;
    type Error = Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Error> {
        SerializeSeq::serialize_element(self, v)
    }
    fn end(self) -> Result<Node, Error> {
        SerializeSeq::end(self)
    }
}
impl SerializeTupleStruct for Sequence {
    type Ok = Node;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Error> {
        SerializeSeq::serialize_element(self, v)
    }
    fn end(self) -> Result<Node, Error> {
        SerializeSeq::end(self)
    }
}
struct VariantSequence(&'static str, Vec<Node>);
impl SerializeTupleVariant for VariantSequence {
    type Ok = Node;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Error> {
        self.1.push(v.serialize(Encoder)?);
        Ok(())
    }
    fn end(self) -> Result<Node, Error> {
        Ok(Node::Map(vec![(
            Node::String(self.0.into()),
            Node::Seq(self.1),
        )]))
    }
}
struct Mapping(Vec<(Node, Node)>, Option<Node>);
impl SerializeMap for Mapping {
    type Ok = Node;
    type Error = Error;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Error> {
        if self.1.is_some() {
            return Err(Error("map key without value".into()));
        }
        self.1 = Some(v.serialize(Encoder)?);
        Ok(())
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), Error> {
        let key = self
            .1
            .take()
            .ok_or_else(|| Error("map value without key".into()))?;
        self.0.push((key, v.serialize(Encoder)?));
        Ok(())
    }
    fn end(self) -> Result<Node, Error> {
        if self.1.is_some() {
            Err(Error("unfinished map entry".into()))
        } else {
            Ok(Node::Map(self.0))
        }
    }
}
impl SerializeStruct for Mapping {
    type Ok = Node;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.0
            .push((Node::String(key.into()), v.serialize(Encoder)?));
        Ok(())
    }
    fn end(self) -> Result<Node, Error> {
        SerializeMap::end(self)
    }
}
struct VariantMapping(&'static str, Vec<(Node, Node)>);
impl SerializeStructVariant for VariantMapping {
    type Ok = Node;
    type Error = Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.1
            .push((Node::String(key.into()), v.serialize(Encoder)?));
        Ok(())
    }
    fn end(self) -> Result<Node, Error> {
        Ok(Node::Map(vec![(
            Node::String(self.0.into()),
            Node::Map(self.1),
        )]))
    }
}

struct Items(std::vec::IntoIter<Node>);
impl<'de> SeqAccess<'de> for Items {
    type Error = Error;
    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        self.0.next().map(|v| seed.deserialize(v)).transpose()
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.0.len())
    }
}
struct Entries(std::vec::IntoIter<(Node, Node)>, Option<Node>);
impl<'de> MapAccess<'de> for Entries {
    type Error = Error;
    fn next_key_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        if self.1.is_some() {
            return Err(Error("map value was not read".into()));
        }
        match self.0.next() {
            Some((k, v)) => {
                self.1 = Some(v);
                seed.deserialize(k).map(Some)
            }
            None => Ok(None),
        }
    }
    fn next_value_seed<T: de::DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Value, Error> {
        seed.deserialize(
            self.1
                .take()
                .ok_or_else(|| Error("map value without key".into()))?,
        )
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.0.len())
    }
}

impl<'de> de::Deserializer<'de> for Node {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Self::Bool(v) => visitor.visit_bool(v),
            Self::I(v) => visitor.visit_i64(v),
            Self::U(v) => visitor.visit_u64(v),
            Self::I128(v) => visitor.visit_i128(v),
            Self::U128(v) => visitor.visit_u128(v),
            Self::F32(v) => visitor.visit_f32(f32::from_bits(v)),
            Self::F64(v) => visitor.visit_f64(f64::from_bits(v)),
            Self::Char(v) => visitor.visit_char(v),
            Self::String(v) => visitor.visit_string(v),
            Self::Bytes(v) => visitor.visit_byte_buf(v),
            Self::Unit => visitor.visit_unit(),
            Self::None => visitor.visit_none(),
            Self::Some(v) => visitor.visit_some(*v),
            Self::Newtype(v) => visitor.visit_newtype_struct(*v),
            Self::Seq(v) => visitor.visit_seq(Items(v.into_iter())),
            Self::Map(v) => visitor.visit_map(Entries(v.into_iter(), None)),
        }
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Self::None => visitor.visit_none(),
            Self::Some(v) => visitor.visit_some(*v),
            other => visitor.visit_some(other),
        }
    }
    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Self::Unit => visitor.visit_unit(),
            other => Err(Error(format!("expected unit, got {other:?}"))),
        }
    }
    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_unit(self, visitor)
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self {
            Self::Newtype(v) => visitor.visit_newtype_struct(*v),
            other => visitor.visit_newtype_struct(other),
        }
    }
    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Self::Seq(v) => visitor.visit_seq(Items(v.into_iter())),
            other => Err(Error(format!("expected sequence, got {other:?}"))),
        }
    }
    fn deserialize_tuple<V: Visitor<'de>>(self, _: usize, visitor: V) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_seq(self, visitor)
    }
    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_seq(self, visitor)
    }
    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self {
            Self::Map(v) => visitor.visit_map(Entries(v.into_iter(), None)),
            other => Err(Error(format!("expected map, got {other:?}"))),
        }
    }
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_map(self, visitor)
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self {
            Self::String(name) => visitor.visit_enum(EnumValue { name, value: None }),
            Self::Map(mut map) if map.len() == 1 => {
                let (key, value) = map.pop().unwrap();
                if let Self::String(name) = key {
                    visitor.visit_enum(EnumValue {
                        name,
                        value: Some(value),
                    })
                } else {
                    Err(Error("enum variant key must be a string".into()))
                }
            }
            other => Err(Error(format!("expected enum, got {other:?}"))),
        }
    }
    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_any(self, visitor)
    }
    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
    }
}

struct EnumValue {
    name: String,
    value: Option<Node>,
}
impl<'de> EnumAccess<'de> for EnumValue {
    type Error = Error;
    type Variant = Self;
    fn variant_seed<V: de::DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Error> {
        let name = seed.deserialize(Node::String(self.name.clone()))?;
        Ok((name, self))
    }
}
impl<'de> VariantAccess<'de> for EnumValue {
    type Error = Error;
    fn unit_variant(self) -> Result<(), Error> {
        if self.value.is_none() {
            Ok(())
        } else {
            Err(Error("expected unit variant".into()))
        }
    }
    fn newtype_variant_seed<T: de::DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        seed.deserialize(
            self.value
                .ok_or_else(|| Error("missing enum value".into()))?,
        )
    }
    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_tuple(
            self.value
                .ok_or_else(|| Error("missing enum value".into()))?,
            len,
            visitor,
        )
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_struct(
            self.value
                .ok_or_else(|| Error("missing enum value".into()))?,
            "",
            fields,
            visitor,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::collections::BTreeMap;

    #[derive(Debug, Serialize, Deserialize)]
    enum Variant {
        Unit,
        New(f32),
        Tuple(u32, f64),
        Record { x: i16 },
    }
    #[derive(Debug, Serialize, Deserialize)]
    struct State {
        floats: [f32; 4],
        wide: [f64; 3],
        keyed: BTreeMap<(u32, u32), Variant>,
        absent: Option<u64>,
        present: Option<u64>,
        bytes: Vec<u8>,
    }
    #[test]
    fn json_tree_round_trips_float_bits_map_keys_and_variants() {
        let state = State {
            floats: [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0],
            wide: [f64::from_bits(0x7ff8_1234_5678_9abc), f64::INFINITY, -0.0],
            keyed: BTreeMap::from([
                ((1, 2), Variant::Unit),
                ((3, 4), Variant::New(f32::NAN)),
                ((5, 6), Variant::Tuple(9, f64::NEG_INFINITY)),
                ((7, 8), Variant::Record { x: -7 }),
            ]),
            absent: None,
            present: Some(u64::MAX),
            bytes: vec![0, 255],
        };
        let value = to_value(&state).unwrap();
        let text = serde_json::to_string(&value).unwrap();
        let decoded: State = from_value(serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!(
            decoded.floats.map(f32::to_bits),
            state.floats.map(f32::to_bits)
        );
        assert_eq!(decoded.wide.map(f64::to_bits), state.wide.map(f64::to_bits));
        assert_eq!(decoded.present, state.present);
        assert!(decoded.absent.is_none());
        assert_eq!(decoded.bytes, state.bytes);
        assert_eq!(decoded.keyed.len(), 4);
        assert!(matches!(decoded.keyed[&(1, 2)], Variant::Unit));
        assert!(matches!(decoded.keyed[&(3,4)],Variant::New(v) if v.is_nan()));
        assert!(matches!(decoded.keyed[&(5,6)],Variant::Tuple(9,v) if v==f64::NEG_INFINITY));
        assert!(matches!(decoded.keyed[&(7, 8)], Variant::Record { x: -7 }));
    }

    #[test]
    fn rejects_malformed_tags_before_deserializing() {
        for value in [
            json!(["f32", "nan"]),
            json!(["map", [["key"]]]),
            json!(["unknown", 1]),
            json!(["unit", 1]),
        ] {
            assert!(from_value::<Value>(value).is_err());
        }
    }

    #[test]
    fn normalizes_only_routing_pointer_fields() {
        let value = json!([
            "seq",
            [
                [
                    "map",
                    [
                        [["string", "width"], ["u", 8]],
                        [["string", "height"], ["u", 8]],
                        [["string", "knowledge_id"], ["u", 5]],
                        [["string", "flags"], ["u", 1234]],
                        [["string", "heights"], ["some", ["u", 5678]]]
                    ]
                ],
                [
                    "map",
                    [
                        [["string", "initial_ptr"], ["u", 1234]],
                        [["string", "flags"], ["u", 1234]]
                    ]
                ]
            ]
        ]);
        let actual = normalize_pointer_fields(value, &BTreeMap::from([(1234, 1)]));
        assert_eq!(actual[1][0][1][3][1], json!(["u", 1]));
        assert_eq!(actual[1][0][1][4][1], json!(["some", ["u", 0]]));
        assert_eq!(actual[1][1][1][0][1], json!(["u", 1]));
        assert_eq!(actual[1][1][1][1][1], json!(["u", 1234]));
    }
}
