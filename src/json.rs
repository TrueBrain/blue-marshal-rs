//! Lossless JSON encoding for [`Value`].
//!
//! Native JSON types are used whenever they round-trip unambiguously
//! (`null`, `bool`, plain integers, finite floats, arrays). Anything that
//! can't be represented as a native JSON field is turned into a *prefixed
//! string*, so plain `serde_json` value files stay human-readable:
//!
//! | Value                         | JSON form                                      |
//! |-------------------------------|------------------------------------------------|
//! | `None`                        | `null`                                         |
//! | `Bool`                        | `true` / `false`                               |
//! | `Int`                         | JSON number                                    |
//! | `Float` (finite)              | JSON number                                    |
//! | `Float` (NaN/Inf)             | `"float:nan"` / `"float:inf"` / `"float:-inf"` |
//! | `Long`                        | `"long:<decimal>"`                             |
//! | `Str` (unicode)               | `"utf8:<text>"`                                |
//! | `Bytes`, valid UTF-8          | `"bytes:<text>"`                               |
//! | `Bytes`, not valid UTF-8      | `"bytes:b64:<base64>"`                         |
//! | `Global`                      | `"global:<module.name>"`                       |
//! | `List`                        | JSON array                                     |
//! | `Tuple`                       | `{"tuple": [...]}`                             |
//! | `Dict`                        | JSON object (see key encoding below)           |
//! | `Instance`                    | `{"instance": {"class": ..., "state": ...}}`   |
//! | `Reduce`                      | `{"reduce": {...}}`                            |
//! | `Callback`                    | `{"callback": ...}`                            |
//!
//! JSON object keys must be strings, but marshal dict keys can be any
//! `Value` (ints, tuples, ...). So dict keys always go through the same
//! prefixed-string encoding used for values, except native types that
//! would normally be bare JSON (null/bool/int/float) are *also* prefixed
//! as keys (`"none"`, `"bool:true"`, `"int:5"`, `"float:1.5"`), and
//! anything else that isn't already a string form is wrapped as
//! `"json:<value-encoded-as-json-text>"`. This keeps key encoding
//! self-describing and unambiguous without needing a second parser.

use crate::error::{Error, Result};
use crate::value::Value;
use base64::Engine;
use num_bigint::BigInt;
use serde_json::{Map, Number, Value as Json};
use std::str::FromStr;

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// Encode a `Value` as a `serde_json::Value`.
pub fn to_json(v: &Value) -> Json {
    match v {
        Value::None => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(i) => Json::Number(Number::from(*i)),
        Value::Float(f) => {
            if f.is_finite() {
                Number::from_f64(*f)
                    .map(Json::Number)
                    .unwrap_or_else(|| Json::String(encode_float(*f)))
            } else {
                Json::String(encode_float(*f))
            }
        }
        Value::Long(n) => Json::String(format!("long:{n}")),
        Value::Str(s) => Json::String(format!("utf8:{s}")),
        Value::Bytes(b) => Json::String(encode_bytes(b)),
        Value::Global(name) => Json::String(format!("global:{name}")),
        Value::List(items) => Json::Array(items.iter().map(to_json).collect()),
        Value::Tuple(items) => {
            let mut m = Map::new();
            m.insert(
                "tuple".into(),
                Json::Array(items.iter().map(to_json).collect()),
            );
            Json::Object(m)
        }
        Value::Dict(pairs) => {
            let mut m = Map::new();
            for (k, val) in pairs {
                m.insert(encode_key(k), to_json(val));
            }
            Json::Object(m)
        }
        Value::Instance { class, state } => {
            let mut inner = Map::new();
            inner.insert("class".into(), Json::String(class.clone()));
            inner.insert("state".into(), to_json(state));
            let mut m = Map::new();
            m.insert("instance".into(), Json::Object(inner));
            Json::Object(m)
        }
        Value::Reduce {
            newobj,
            callable,
            args,
            state,
            list_items,
            dict_items,
        } => {
            let mut inner = Map::new();
            inner.insert("newobj".into(), Json::Bool(*newobj));
            inner.insert("callable".into(), to_json(callable));
            inner.insert("args".into(), to_json(args));
            inner.insert(
                "state".into(),
                state.as_ref().map(|s| to_json(s)).unwrap_or(Json::Null),
            );
            inner.insert(
                "list_items".into(),
                Json::Array(list_items.iter().map(to_json).collect()),
            );
            inner.insert(
                "dict_items".into(),
                Json::Array(
                    dict_items
                        .iter()
                        .map(|(k, v)| Json::Array(vec![to_json(k), to_json(v)]))
                        .collect(),
                ),
            );
            let mut m = Map::new();
            m.insert("reduce".into(), Json::Object(inner));
            Json::Object(m)
        }
        Value::Callback(inner) => {
            let mut m = Map::new();
            m.insert("callback".into(), to_json(inner));
            Json::Object(m)
        }
    }
}

/// Decode a `serde_json::Value` back into a `Value`.
pub fn from_json(j: &Json) -> Result<Value> {
    match j {
        Json::Null => Ok(Value::None),
        Json::Bool(b) => Ok(Value::Bool(*b)),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Int(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Float(f))
            } else {
                Err(Error::Json(format!("unrepresentable number {n}")))
            }
        }
        Json::String(s) => decode_string(s),
        Json::Array(items) => Ok(Value::List(
            items.iter().map(from_json).collect::<Result<Vec<_>>>()?,
        )),
        Json::Object(m) => decode_object(m),
    }
}

fn encode_float(f: f64) -> String {
    if f.is_nan() {
        "float:nan".into()
    } else if f == f64::INFINITY {
        "float:inf".into()
    } else if f == f64::NEG_INFINITY {
        "float:-inf".into()
    } else {
        format!("float:{f}")
    }
}

fn decode_float(rest: &str) -> Result<f64> {
    match rest {
        "nan" => Ok(f64::NAN),
        "inf" => Ok(f64::INFINITY),
        "-inf" => Ok(f64::NEG_INFINITY),
        _ => f64::from_str(rest).map_err(|_| Error::Json(format!("bad float literal {rest:?}"))),
    }
}

/// `"bytes:<text>"` if the bytes are valid UTF-8 and don't collide with the
/// `b64:` escape marker, `"bytes:b64:<base64>"` otherwise.
fn encode_bytes(b: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(b) {
        if !s.starts_with("b64:") {
            return format!("bytes:{s}");
        }
    }
    format!("bytes:b64:{}", b64().encode(b))
}

fn decode_bytes(rest: &str) -> Result<Vec<u8>> {
    if let Some(b64_part) = rest.strip_prefix("b64:") {
        b64()
            .decode(b64_part)
            .map_err(|e| Error::Json(format!("bad base64 in bytes: {e}")))
    } else {
        Ok(rest.as_bytes().to_vec())
    }
}

/// Encode a dict *key* value. Keys must always be strings in JSON, so even
/// values that would normally be a native JSON type (null/bool/int/float)
/// get an explicit prefix here.
fn encode_key(v: &Value) -> String {
    match v {
        Value::None => "none".to_string(),
        Value::Bool(b) => format!("bool:{b}"),
        Value::Int(i) => format!("int:{i}"),
        Value::Float(f) => encode_float(*f),
        Value::Long(n) => format!("long:{n}"),
        Value::Str(s) => format!("utf8:{s}"),
        Value::Bytes(b) => encode_bytes(b),
        Value::Global(name) => format!("global:{name}"),
        other => format!("json:{}", serde_json::to_string(&to_json(other)).unwrap()),
    }
}

fn decode_key(s: &str) -> Result<Value> {
    decode_string(s)
}

/// Shared string-decoding logic for both plain values and dict keys: parse
/// the marker prefix and dispatch.
fn decode_string(s: &str) -> Result<Value> {
    if let Some(rest) = s.strip_prefix("long:") {
        return BigInt::from_str(rest)
            .map(Value::Long)
            .map_err(|_| Error::Json(format!("bad long literal {rest:?}")));
    }
    if let Some(rest) = s.strip_prefix("bytes:") {
        return decode_bytes(rest).map(Value::Bytes);
    }
    if let Some(rest) = s.strip_prefix("utf8:") {
        return Ok(Value::Str(rest.to_string()));
    }
    if let Some(rest) = s.strip_prefix("global:") {
        return Ok(Value::Global(rest.to_string()));
    }
    if let Some(rest) = s.strip_prefix("float:") {
        return decode_float(rest).map(Value::Float);
    }
    if let Some(rest) = s.strip_prefix("bool:") {
        return match rest {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(Error::Json(format!("bad bool literal {rest:?}"))),
        };
    }
    if let Some(rest) = s.strip_prefix("int:") {
        return rest
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| Error::Json(format!("bad int literal {rest:?}")));
    }
    if s == "none" {
        return Ok(Value::None);
    }
    if let Some(rest) = s.strip_prefix("json:") {
        let j: Json =
            serde_json::from_str(rest).map_err(|e| Error::Json(format!("bad nested json: {e}")))?;
        return from_json(&j);
    }
    Err(Error::Json(format!(
        "string value/key without a recognized prefix: {s:?}"
    )))
}

fn decode_object(m: &Map<String, Json>) -> Result<Value> {
    if m.len() == 1 {
        if let Some(Json::Array(items)) = m.get("tuple") {
            return Ok(Value::Tuple(
                items.iter().map(from_json).collect::<Result<Vec<_>>>()?,
            ));
        }
        if let Some(Json::Object(inner)) = m.get("instance") {
            let class = inner
                .get("class")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::Json("instance missing class".into()))?
                .to_string();
            let state = inner
                .get("state")
                .ok_or_else(|| Error::Json("instance missing state".into()))?;
            return Ok(Value::Instance {
                class,
                state: Box::new(from_json(state)?),
            });
        }
        if let Some(Json::Object(inner)) = m.get("reduce") {
            let newobj = inner
                .get("newobj")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let callable = from_json(
                inner
                    .get("callable")
                    .ok_or_else(|| Error::Json("reduce missing callable".into()))?,
            )?;
            let args = from_json(
                inner
                    .get("args")
                    .ok_or_else(|| Error::Json("reduce missing args".into()))?,
            )?;
            let state = match inner.get("state") {
                Some(Json::Null) | None => None,
                Some(v) => Some(Box::new(from_json(v)?)),
            };
            let list_items = match inner.get("list_items") {
                Some(Json::Array(items)) => {
                    items.iter().map(from_json).collect::<Result<Vec<_>>>()?
                }
                _ => Vec::new(),
            };
            let dict_items = match inner.get("dict_items") {
                Some(Json::Array(items)) => items
                    .iter()
                    .map(|pair| match pair {
                        Json::Array(kv) if kv.len() == 2 => {
                            Ok((from_json(&kv[0])?, from_json(&kv[1])?))
                        }
                        _ => Err(Error::Json("bad dict_items entry".into())),
                    })
                    .collect::<Result<Vec<_>>>()?,
                _ => Vec::new(),
            };
            return Ok(Value::Reduce {
                newobj,
                callable: Box::new(callable),
                args: Box::new(args),
                state,
                list_items,
                dict_items,
            });
        }
        if let Some(inner) = m.get("callback") {
            return Ok(Value::Callback(Box::new(from_json(inner)?)));
        }
    }

    let mut pairs = Vec::with_capacity(m.len());
    for (k, v) in m {
        pairs.push((decode_key(k)?, from_json(v)?));
    }
    Ok(Value::Dict(pairs))
}
