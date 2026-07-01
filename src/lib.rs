//! A Rust port of the `blue.Marshal` binary object format used by EVE
//! Online (see `src/Marshal.cpp` in https://github.com/carbonengine/blue,
//! the "release/2.x" branch), plus a lossless JSON encoding for the
//! decoded values.
//!
//! ```no_run
//! let bytes: Vec<u8> = std::fs::read("some.marshal").unwrap();
//! let decoded = blue_marshal::decode(&bytes).unwrap();
//! let json = blue_marshal::to_json(&decoded.value);
//! println!("{}", serde_json::to_string_pretty(&json).unwrap());
//!
//! let value = blue_marshal::from_json(&json).unwrap();
//! let reencoded = blue_marshal::encode(&value, &blue_marshal::EncodeOptions::default()).unwrap();
//! ```
//!
//! ## Scope
//!
//! This is a faithful port of the core object graph (None/bool/int/long/
//! float/bytes/unicode/tuple/list/dict/global/old-style instances/the
//! `__reduce__` protocol/callback wrapper) plus reference resolution.
//! It intentionally does **not** implement:
//!
//! - `TY_DBROW`: EVE's custom row format (has its own recursive
//!   schema+data sub-encoding, out of scope for this port).
//! - `TY_WSTREAM`: a nested pre-built `blue.MarshalStream` blob.
//! - `TY_PICKLE` / `TY_PICKLER`: the generic `cPickle` fallback path.
//!
//! Decoding a stream containing any of these tags returns
//! `Error::Unsupported`. None of them are ever produced by the encoder.
//!
//! ## Shared references
//!
//! - **Decoding** resolves `TY_REFERENCE` by inlining a clone of the
//!   referenced value at every point it's referenced, so the returned
//!   `Value` tree is reference-free. A container that references *itself*
//!   before it has finished being read (a genuine cycle) cannot be
//!   represented as an owned Rust tree; in that pathological case the
//!   reference resolves to `Value::None` instead of infinitely
//!   recursing. `Marshal.cpp` itself explicitly forbids direct
//!   self-referencing tuples, and true cycles do not occur in ordinary
//!   EVE configuration data, so this is not expected to matter in
//!   practice.
//! - **Encoding** never tracks or emits shared references: every
//!   occurrence of a repeated value is written out in full. This is
//!   always valid to read back. The `TY_REFERENCE` is a size optimization,
//!   not something the format requires: it just means re-encoded
//!   streams may be larger than what `Marshal.cpp` itself would produce.

mod adler32;
mod error;
mod json;
mod reader;
mod strtable;
mod tags;
mod value;
mod writer;

pub use error::{Error, Result};
pub use json::{from_json, to_json};
pub use reader::{decode, Decoded};
pub use value::Value;
pub use writer::{encode, EncodeOptions};

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigInt;

    fn assert_value_eq(a: &Value, b: &Value) {
        if let (Value::Float(x), Value::Float(y)) = (a, b) {
            if x.is_nan() && y.is_nan() {
                return;
            }
        }
        assert_eq!(a, b);
    }

    fn roundtrip_binary(v: Value) {
        let bytes = encode(&v, &EncodeOptions::default()).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_value_eq(&decoded.value, &v);
        assert!(decoded.had_crc);
    }

    fn roundtrip_json(v: Value) {
        let j = to_json(&v);
        let back = from_json(&j).unwrap();
        assert_value_eq(&back, &v);
    }

    fn sample_values() -> Vec<Value> {
        vec![
            Value::None,
            Value::Bool(true),
            Value::Bool(false),
            Value::Int(0),
            Value::Int(-1),
            Value::Int(1),
            Value::Int(127),
            Value::Int(-128),
            Value::Int(40000),
            Value::Int(i64::MAX),
            Value::Int(i64::MIN),
            Value::Long(BigInt::from(0)),
            Value::Long(BigInt::parse_bytes(b"123456789012345678901234567890", 10).unwrap()),
            Value::Long(-BigInt::parse_bytes(b"123456789012345678901234567890", 10).unwrap()),
            Value::Float(0.0),
            Value::Float(-0.0),
            Value::Float(3.5),
            Value::Float(f64::NAN),
            Value::Float(f64::INFINITY),
            Value::Float(f64::NEG_INFINITY),
            Value::Bytes(vec![]),
            Value::Bytes(vec![b'a']),
            Value::Bytes(b"hello world".to_vec()),
            Value::Bytes(vec![0xff, 0x00, 0xfe, 0x80]),
            Value::Bytes(b"b64:not-really-base64".to_vec()),
            Value::Str(String::new()),
            Value::Str("hi".into()),
            Value::Str("unicode \u{1F600} snowman \u{2603}".into()),
            Value::Global("__builtin__.dict".into()),
            Value::Tuple(vec![]),
            Value::Tuple(vec![Value::Int(1)]),
            Value::Tuple(vec![Value::Int(1), Value::Int(2)]),
            Value::Tuple(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
            Value::List(vec![]),
            Value::List(vec![Value::Int(1)]),
            Value::List(vec![Value::Int(1), Value::Str("x".into())]),
            Value::Dict(vec![
                (Value::Str("a".into()), Value::Int(1)),
                (Value::Int(5), Value::Bool(true)),
                (
                    Value::Tuple(vec![Value::Int(1), Value::Int(2)]),
                    Value::None,
                ),
            ]),
            Value::Instance {
                class: "foo.Bar".into(),
                state: Box::new(Value::Dict(vec![(Value::Str("x".into()), Value::Int(1))])),
            },
            Value::Reduce {
                newobj: false,
                callable: Box::new(Value::Global("copy_reg._reconstructor".into())),
                args: Box::new(Value::Tuple(vec![Value::Global("foo.Bar".into())])),
                state: Some(Box::new(Value::Dict(vec![(
                    Value::Str("y".into()),
                    Value::Int(2),
                )]))),
                list_items: vec![Value::Int(1), Value::Int(2)],
                dict_items: vec![(Value::Str("k".into()), Value::Int(9))],
            },
            Value::Reduce {
                newobj: true,
                callable: Box::new(Value::None),
                args: Box::new(Value::Tuple(vec![Value::Global("foo.Baz".into())])),
                state: None,
                list_items: vec![],
                dict_items: vec![],
            },
            Value::Callback(Box::new(Value::Int(42))),
        ]
    }

    #[test]
    fn binary_roundtrip() {
        for v in sample_values() {
            roundtrip_binary(v);
        }
    }

    #[test]
    fn json_roundtrip() {
        for v in sample_values() {
            roundtrip_json(v);
        }
    }

    #[test]
    fn nested_structure_roundtrips_both_ways() {
        let v = Value::Dict(vec![(
            Value::Str("items".into()),
            Value::List(vec![
                Value::Tuple(vec![Value::Int(1), Value::Str("one".into())]),
                Value::Tuple(vec![Value::Int(2), Value::Str("two".into())]),
                Value::Long(BigInt::parse_bytes(b"99999999999999999999", 10).unwrap()),
                Value::Bytes(vec![0, 159, 146, 150]),
            ]),
        )]);
        roundtrip_binary(v.clone());
        roundtrip_json(v);
    }

    #[test]
    fn string_table_reads_back() {
        // TY_STR_TABLE, index 1 == "*corpid"
        let bytes = [tags_test::TY_SIGNATURE2, 1, tags_test::TY_STR_TABLE, 1];
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.value, Value::Bytes(b"*corpid".to_vec()));
    }

    mod tags_test {
        pub const TY_SIGNATURE2: u8 = 125;
        pub const TY_STR_TABLE: u8 = 17;
        pub const TY_DICT: u8 = 22;
        pub const TY_TUPLE: u8 = 20;
        pub const TY_REFERENCE: u8 = 27;
    }

    #[test]
    fn empty_input_is_eof() {
        assert!(matches!(decode(&[]), Err(Error::Eof)));
    }

    #[test]
    fn bad_signature_is_rejected() {
        let bytes = [0u8; 4];
        assert!(matches!(decode(&bytes), Err(Error::Invalid(_))));
    }

    #[test]
    fn truncated_header_is_eof() {
        // TY_SIGNATURE2 with no version byte following.
        let bytes = [tags_test::TY_SIGNATURE2];
        assert!(matches!(decode(&bytes), Err(Error::Eof)));
    }

    #[test]
    fn negative_tuple_length_is_rejected() {
        // TY_TUPLE with a length encoded as -1 (0xFF marker + i32 LE -1).
        let mut bytes = vec![tags_test::TY_SIGNATURE2, 1, tags_test::TY_TUPLE, 0xFF];
        bytes.extend_from_slice(&(-1i32).to_le_bytes());
        assert!(matches!(decode(&bytes), Err(Error::Invalid(_))));
    }

    #[test]
    fn huge_dict_length_with_short_buffer_fails_fast_without_large_allocation() {
        // TY_DICT claiming i32::MAX pairs, but the buffer ends right after
        // the length - must be rejected via a bounds check, not by trying
        // to allocate a multi-gigabyte Vec.
        let mut bytes = vec![tags_test::TY_SIGNATURE2, 1, tags_test::TY_DICT, 0xFF];
        bytes.extend_from_slice(&i32::MAX.to_le_bytes());
        assert!(matches!(decode(&bytes), Err(Error::Eof)));
    }

    #[test]
    fn dangling_reference_is_rejected() {
        // TY_REFERENCE to an id that was never registered.
        let mut bytes = vec![tags_test::TY_SIGNATURE2, 1, tags_test::TY_REFERENCE, 0xFF];
        bytes.extend_from_slice(&42i32.to_le_bytes());
        assert!(matches!(decode(&bytes), Err(Error::BadReference(42))));
    }

    #[test]
    fn deeply_nested_value_hits_recursion_limit_on_encode() {
        let mut v = Value::None;
        for _ in 0..2000 {
            v = Value::List(vec![v]);
        }
        assert!(matches!(
            encode(&v, &EncodeOptions::default()),
            Err(Error::RecursionLimit)
        ));
    }

    #[test]
    fn crc_mismatch_is_detected() {
        let mut bytes = encode(&Value::Int(5), &EncodeOptions::default()).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        assert!(matches!(decode(&bytes), Err(Error::ChecksumMismatch)));
    }
}
