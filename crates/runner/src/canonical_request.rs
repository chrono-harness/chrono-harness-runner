//! Canonical JSON for the already typed request projection. Keep large
//! observations borrowed and write containers directly into one output buffer.
#[cfg(not(verus_keep_ghost))]
use serde::Serialize;
#[cfg(not(verus_keep_ghost))]
use serde_json::Value;

#[cfg(not(verus_keep_ghost))]
pub(crate) fn encode<T: Serialize>(header: &T, observations: &Value) -> Result<Vec<u8>, String> {
    let header = serde_json::to_value(header).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    object(
        header
            .as_object()
            .ok_or("request projection must be an object")?
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .chain(std::iter::once(("observations", observations))),
        &mut out,
    )?;
    Ok(out)
}
#[cfg(not(verus_keep_ghost))]
fn object<'a>(
    entries: impl Iterator<Item = (&'a str, &'a Value)>,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    let mut entries: Vec<_> = entries.collect();
    // RFC 8785 orders unescaped property names by UTF-16 code units.
    entries.sort_unstable_by(|(a, _), (b, _)| a.encode_utf16().cmp(b.encode_utf16()));
    out.push(b'{');
    for (i, (key, value)) in entries.into_iter().enumerate() {
        if i != 0 {
            out.push(b',');
        }
        serde_json::to_writer(&mut *out, key).map_err(|e| e.to_string())?;
        out.push(b':');
        write(value, out)?;
    }
    out.push(b'}');
    Ok(())
}
#[cfg(not(verus_keep_ghost))]
fn write(value: &Value, out: &mut Vec<u8>) -> Result<(), String> {
    match value {
        Value::Object(values) => object(values.iter().map(|(k, v)| (k.as_str(), v)), out),
        Value::Array(values) => {
            out.push(b'[');
            for (i, value) in values.iter().enumerate() {
                if i != 0 {
                    out.push(b',');
                }
                write(value, out)?;
            }
            out.push(b']');
            Ok(())
        }
        // These integers have exactly the same decimal spelling in JSON and
        // ECMAScript. In particular, original byte arrays avoid per-element
        // floating point formatting and the JCS formatter's boxed writers.
        Value::Number(n) if n.as_i64().is_some_and(direct_integer) => {
            serde_json::to_writer(out, n).map_err(|e| e.to_string())
        }
        // The fixed dependency still owns every floating point/large integer
        // normalization, including negative zero and precision boundaries.
        Value::Number(n) => serde_jcs::to_writer(out, &n.as_f64().ok_or("invalid JSON number")?)
            .map_err(|e| e.to_string()),
        _ => serde_json::to_writer(out, value).map_err(|e| e.to_string()),
    }
}

// serde_json's Value keeps raw JSON objects and integers intact. Capture the
// Serde types that JSON would erase before applying the v1 binary64 profile.
#[cfg(not(verus_keep_ghost))]
use serde_content::{Data, Number, Value as Content};

#[cfg(not(verus_keep_ghost))]
pub(crate) fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut source = serde_content::Serializer::new()
        .human_readable()
        .serialize(value)
        .map_err(|error| error.to_string())?;
    match &source {
        Content::Number(Number::F32(n)) if !n.is_finite() => {
            return Err("invalid float value".into());
        }
        Content::Number(Number::F64(n)) if !n.is_finite() => {
            return Err("invalid float value".into());
        }
        Content::Number(Number::I128(_)) => return Err("i128 is not supported".into()),
        Content::Number(Number::U128(_)) => return Err("u128 is not supported".into()),
        _ => {}
    }
    prepare(&mut source, false)?;
    let value = serde_json::to_value(source).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    write(&value, &mut bytes)?;
    Ok(bytes)
}

// Preserve typed numeric map keys before serde_json converts them to strings.
// RawValue is replayed through serde_json's native Value serializer, including
// when its JSON contains an ordinary map with serde_json's private field name.
#[cfg(not(verus_keep_ghost))]
fn prepare(value: &mut Content<'_>, key: bool) -> Result<(), String> {
    match value {
        Content::Number(number) => {
            let binary64 = match *number {
                Number::I64(n) => n as f64,
                Number::U64(n) => n as f64,
                Number::I128(n) => n as f64,
                Number::U128(n) => n as f64,
                Number::F32(n) => f64::from(n),
                Number::F64(n) => n,
                _ => return Ok(()),
            };
            if key {
                *value = Content::String(
                    serde_jcs::to_string(&binary64)
                        .map_err(|error| error.to_string())?
                        .into(),
                );
            } else if matches!(number, Number::I128(_) | Number::U128(_) | Number::F32(_)) {
                *number = Number::F64(binary64);
            }
        }
        Content::Seq(values) | Content::Tuple(values) => {
            for value in values {
                prepare(value, false)?;
            }
        }
        Content::Map(entries) => {
            for (key, value) in entries {
                prepare(key, true)?;
                prepare(value, false)?;
            }
        }
        Content::Option(Some(value)) => prepare(value, key)?,
        Content::Struct(definition) => prepare_data(&mut definition.data, key)?,
        Content::Enum(variant) => prepare_data(&mut variant.data, false)?,
        _ => {}
    }
    Ok(())
}

#[cfg(not(verus_keep_ghost))]
fn prepare_data(data: &mut Data<'_>, key: bool) -> Result<(), String> {
    match data {
        Data::Unit => {}
        Data::NewType { value } => prepare(value, key)?,
        Data::Tuple { values } => {
            for value in values {
                prepare(value, false)?;
            }
        }
        Data::Struct { fields } => {
            for (_, value) in fields {
                prepare(value, false)?;
            }
        }
    }
    Ok(())
}

#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
/// Within these endpoints consecutive integer values have exact binary64 representations.
pub(crate) fn direct_integer(n: i64) -> (direct: bool)
    ensures direct == (-9_007_199_254_740_992 <= n && n <= 9_007_199_254_740_992),
{
    -9_007_199_254_740_992i64 <= n && n <= 9_007_199_254_740_992i64
}
}
