// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Runtime conversion between protobuf and JSON, using the descriptors. The
//! harness names no generated type, so a field or RPC being removed can't stop
//! it compiling: requests skip what's gone, and output just lacks it.

use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::Path;

use anyhow::anyhow;
use anyhow::bail;
use anyhow::Result;
use prost_reflect::DynamicMessage;
use prost_reflect::FieldDescriptor;
use prost_reflect::Kind;
use prost_reflect::MapKey;
use prost_reflect::MessageDescriptor;
use prost_reflect::Value as Pv;
use serde_json::Map;
use serde_json::Number;
use serde_json::Value;

pub fn splitmix(state: u64) -> u64 {
    let mut z = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Seeded generator for the harness's own decisions.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        splitmix(self.0)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    pub fn chance(&mut self, percent_times_ten: u64) -> bool {
        self.below(1000) < percent_times_ten
    }
}

pub fn float(val: f64) -> Value {
    Number::from_f64(val)
        .map(Value::Number)
        .unwrap_or_else(|| Value::String(val.to_string()))
}

/// The shortest form that round-trips an f32, not a widened double.
pub fn float32(val: f32) -> Value {
    val.to_string()
        .parse()
        .map(float)
        .unwrap_or_else(|_| Value::String(val.to_string()))
}

pub fn message_to_json(message: &DynamicMessage) -> Value {
    let mut out = Map::new();
    for (field, value) in message.fields() {
        out.insert(field.name().to_string(), value_to_json(&field, value));
    }
    Value::Object(out)
}

fn value_to_json(field: &FieldDescriptor, value: &Pv) -> Value {
    match value {
        Pv::Bool(v) => Value::Bool(*v),
        Pv::I32(v) => (*v).into(),
        Pv::I64(v) => (*v).into(),
        Pv::U32(v) => (*v).into(),
        Pv::U64(v) => (*v).into(),
        Pv::F32(v) => float32(*v),
        Pv::F64(v) => float(*v),
        Pv::String(v) => Value::String(v.clone()),
        Pv::Bytes(v) => Value::String(hex(v)),
        Pv::EnumNumber(number) => match field.kind() {
            Kind::Enum(descriptor) => descriptor
                .get_value(*number)
                .map(|v| Value::String(v.name().to_string()))
                .unwrap_or_else(|| (*number).into()),
            _ => (*number).into(),
        },
        Pv::Message(v) => message_to_json(v),
        Pv::List(values) => Value::Array(values.iter().map(|v| value_to_json(field, v)).collect()),
        Pv::Map(values) => {
            let value_field = match field.kind() {
                Kind::Message(entry) => entry.map_entry_value_field(),
                _ => return Value::Null,
            };
            let mut out = Map::new();
            for (key, value) in values {
                out.insert(map_key(key), value_to_json(&value_field, value));
            }
            Value::Object(out)
        }
    }
}

fn map_key(key: &MapKey) -> String {
    match key {
        MapKey::Bool(v) => v.to_string(),
        MapKey::I32(v) => v.to_string(),
        MapKey::I64(v) => v.to_string(),
        MapKey::U32(v) => v.to_string(),
        MapKey::U64(v) => v.to_string(),
        MapKey::String(v) => v.clone(),
    }
}

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn unhex(text: &str) -> Result<Vec<u8>> {
    (0..text.len() / 2)
        .map(|i| Ok(u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)?))
        .collect()
}

/// Fields the message no longer has are added to `skipped`.
pub fn json_to_message(
    descriptor: &MessageDescriptor,
    json: &Value,
    skipped: &mut Vec<String>,
) -> Result<DynamicMessage> {
    let mut message = DynamicMessage::new(descriptor.clone());
    let object = json
        .as_object()
        .ok_or_else(|| anyhow!("{} needs an object", descriptor.full_name()))?;
    for (key, value) in object {
        let Some(field) = descriptor.get_field_by_name(key) else {
            skipped.push(format!("{}.{key}", descriptor.full_name()));
            continue;
        };
        message.set_field(&field, json_to_value(&field, value, skipped)?);
    }
    Ok(message)
}

fn json_to_value(field: &FieldDescriptor, json: &Value, skipped: &mut Vec<String>) -> Result<Pv> {
    if field.is_map() {
        bail!("map fields can't be sent: {}", field.full_name());
    }
    if field.is_list() {
        let items = json
            .as_array()
            .ok_or_else(|| anyhow!("{} needs an array", field.full_name()))?;
        return Ok(Pv::List(
            items
                .iter()
                .map(|item| single(field, item, skipped))
                .collect::<Result<_>>()?,
        ));
    }
    single(field, json, skipped)
}

fn single(field: &FieldDescriptor, json: &Value, skipped: &mut Vec<String>) -> Result<Pv> {
    let bad = || anyhow!("bad value for {}: {json}", field.full_name());
    Ok(match field.kind() {
        Kind::Bool => Pv::Bool(json.as_bool().ok_or_else(bad)?),
        Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => {
            Pv::I32(json.as_i64().ok_or_else(bad)? as i32)
        }
        Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => Pv::I64(json.as_i64().ok_or_else(bad)?),
        Kind::Uint32 | Kind::Fixed32 => Pv::U32(json.as_u64().ok_or_else(bad)? as u32),
        Kind::Uint64 | Kind::Fixed64 => Pv::U64(json.as_u64().ok_or_else(bad)?),
        Kind::Float => Pv::F32(match json {
            Value::Number(_) => json.to_string().parse().map_err(|_| bad())?,
            Value::String(text) => text.parse().map_err(|_| bad())?,
            _ => return Err(bad()),
        }),
        Kind::Double => Pv::F64(json.as_f64().ok_or_else(bad)?),
        Kind::String => Pv::String(json.as_str().ok_or_else(bad)?.to_string()),
        Kind::Bytes => Pv::Bytes(unhex(json.as_str().ok_or_else(bad)?)?.into()),
        Kind::Enum(descriptor) => Pv::EnumNumber(match json {
            Value::String(name) => descriptor.get_value_by_name(name).ok_or_else(bad)?.number(),
            _ => json.as_i64().ok_or_else(bad)? as i32,
        }),
        Kind::Message(descriptor) => Pv::Message(json_to_message(&descriptor, json, skipped)?),
    })
}

/// Sections on their own line, and the entries directly inside them too, so
/// plain `diff` gives one line per card or event.
pub fn write_output(path: &Path, sections: &Map<String, Value>) -> Result<()> {
    let mut out = std::io::BufWriter::new(fs::File::create(path)?);
    writeln!(out, "{{")?;
    let mut first_section = true;
    for (name, value) in sections {
        if !first_section {
            writeln!(out, ",")?;
        }
        first_section = false;
        write!(out, "{}: ", serde_json::to_string(name)?)?;
        match value {
            Value::Object(items) if !items.is_empty() => {
                writeln!(out, "{{")?;
                let mut first = true;
                for (key, item) in items {
                    if !first {
                        writeln!(out, ",")?;
                    }
                    first = false;
                    write!(out, "{}: {}", serde_json::to_string(key)?, item)?;
                }
                write!(out, "\n}}")?;
            }
            Value::Array(items) if !items.is_empty() => {
                writeln!(out, "[")?;
                for (idx, item) in items.iter().enumerate() {
                    if idx > 0 {
                        writeln!(out, ",")?;
                    }
                    write!(out, "{item}")?;
                }
                write!(out, "\n]")?;
            }
            other => write!(out, "{other}")?,
        }
    }
    writeln!(out, "\n}}")?;
    out.flush()?;
    Ok(())
}
