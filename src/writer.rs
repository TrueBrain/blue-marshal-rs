use crate::adler32::adler32;
use crate::error::{Error, Result};
use crate::tags::*;
use crate::value::Value;
use num_bigint::BigInt;
use num_traits::Zero;

/// Options controlling how a `Value` is serialized to the wire format.
pub struct EncodeOptions {
    /// Stream format version. This port always writes version 1
    /// (`TY_SIGNATURE2`), since version 0 additionally requires a
    /// trailing shared-object index map that only makes sense when shared
    /// references are tracked - which we intentionally never do (see
    /// `README.md`: "encoding skips shared references"). `Marshal.cpp`
    /// reads both versions natively, so this is a safe default.
    pub version: u8,
    /// Whether to append a `TY_CRC_CHECK` (adler32) header, verified by
    /// `Marshal.cpp` on load unless `skipCrcCheck` is passed.
    pub checksum: bool,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        EncodeOptions {
            version: 1,
            checksum: true,
        }
    }
}

/// Encode a `Value` into a `Marshal.cpp`-compatible byte stream.
///
/// Shared references are never emitted: every occurrence of a repeated
/// value is written out in full (this is what the task calls "skip shared
/// references" on the encode side). `Marshal.cpp`'s reader has no problem
/// with this - `TY_REFERENCE` is purely an optional space optimization, not
/// something the format requires.
pub fn encode(value: &Value, opts: &EncodeOptions) -> Result<Vec<u8>> {
    let mut w = Writer {
        buf: Vec::new(),
        recursion: 0,
    };

    if opts.version == 0 {
        w.buf.push(TY_SIGNATURE);
        w.buf.extend_from_slice(&0i32.to_le_bytes()); // mapcount = 0, nothing shared
    } else {
        w.buf.push(TY_SIGNATURE2);
        w.buf.push(opts.version);
    }

    let crc_pos = if opts.checksum {
        w.write_raw_type(TY_CRC_CHECK)?;
        let pos = w.buf.len();
        w.buf.extend_from_slice(&0i32.to_le_bytes()); // placeholder
        Some(pos)
    } else {
        None
    };

    w.write_value(value)?;

    if let Some(crc_pos) = crc_pos {
        let start = crc_pos + 4;
        let a32 = adler32(1, &w.buf[start..]) as i32;
        w.buf[crc_pos..crc_pos + 4].copy_from_slice(&a32.to_le_bytes());
    }

    Ok(w.buf)
}

struct Writer {
    buf: Vec<u8>,
    recursion: u32,
}

impl Writer {
    fn write_raw_type(&mut self, t: u8) -> Result<()> {
        self.buf.push(t);
        Ok(())
    }

    fn write_u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    /// Matches `WriteStream::WriteInteger`.
    fn write_integer(&mut self, v: i32) {
        if (0..(u8::MAX as i32)).contains(&v) {
            self.buf.push(v as u8);
        } else {
            self.buf.push(u8::MAX);
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
    }

    /// Matches `WriteStream::WriteBuff`: length-prefixed bytes.
    fn write_buff(&mut self, data: &[u8]) {
        self.write_integer(data.len() as i32);
        self.buf.extend_from_slice(data);
    }

    fn write_value(&mut self, v: &Value) -> Result<()> {
        self.recursion += 1;
        if self.recursion > RECURSION_LIMIT {
            return Err(Error::RecursionLimit);
        }
        let r = self.write_value_inner(v);
        self.recursion -= 1;
        r
    }

    fn write_value_inner(&mut self, v: &Value) -> Result<()> {
        match v {
            Value::None => self.write_raw_type(TY_NONE),
            Value::Bool(true) => self.write_raw_type(TY_TRUE),
            Value::Bool(false) => self.write_raw_type(TY_FALSE),
            Value::Int(i) => self.write_int(*i),
            Value::Long(n) => self.write_long(n),
            Value::Float(f) => self.write_float(*f),
            Value::Bytes(b) => self.write_bytes(b),
            Value::Str(s) => self.write_str(s),
            Value::Global(name) => {
                self.write_raw_type(TY_GLOBAL)?;
                self.write_buff(name.as_bytes());
                Ok(())
            }
            Value::Tuple(items) => self.write_tuple(items),
            Value::List(items) => self.write_list(items),
            Value::Dict(pairs) => self.write_dict(pairs),
            Value::Instance { class, state } => {
                self.write_raw_type(TY_INSTANCE)?;
                // The guid is written as a plain string (see
                // Marshal::WriteObjectInstance), not TY_GLOBAL.
                self.write_bytes(class.as_bytes())?;
                self.write_value(state)
            }
            Value::Reduce {
                newobj,
                callable,
                args,
                state,
                list_items,
                dict_items,
            } => self.write_reduce(
                *newobj,
                callable,
                args,
                state.as_deref(),
                list_items,
                dict_items,
            ),
            Value::Callback(inner) => {
                self.write_raw_type(TY_CALLBACK)?;
                self.write_value(inner)
            }
        }
    }

    fn write_int(&mut self, i: i64) -> Result<()> {
        match i {
            -1 => self.write_raw_type(TY_INT_N1),
            0 => self.write_raw_type(TY_INT_0),
            1 => self.write_raw_type(TY_INT_1),
            _ if i >= i8::MIN as i64 && i <= i8::MAX as i64 => {
                self.write_raw_type(TY_INT8)?;
                self.write_u8(i as i8 as u8);
                Ok(())
            }
            _ if i >= i16::MIN as i64 && i <= i16::MAX as i64 => {
                self.write_raw_type(TY_INT16)?;
                self.buf.extend_from_slice(&(i as i16).to_le_bytes());
                Ok(())
            }
            _ if i >= i32::MIN as i64 && i <= i32::MAX as i64 => {
                self.write_raw_type(TY_INT32)?;
                self.buf.extend_from_slice(&(i as i32).to_le_bytes());
                Ok(())
            }
            _ => {
                self.write_raw_type(TY_INT64)?;
                self.buf.extend_from_slice(&i.to_le_bytes());
                Ok(())
            }
        }
    }

    fn write_long(&mut self, n: &BigInt) -> Result<()> {
        self.write_raw_type(TY_LONG)?;
        if n.is_zero() {
            self.write_buff(&[]);
        } else {
            self.write_buff(&n.to_signed_bytes_le());
        }
        Ok(())
    }

    fn write_float(&mut self, f: f64) -> Result<()> {
        if f == 0.0 {
            self.write_raw_type(TY_FLOAT_0)
        } else {
            self.write_raw_type(TY_FLOAT)?;
            self.buf.extend_from_slice(&f.to_le_bytes());
            Ok(())
        }
    }

    fn write_bytes(&mut self, b: &[u8]) -> Result<()> {
        if b.is_empty() {
            self.write_raw_type(TY_STR_EMPTY)
        } else if b.len() == 1 {
            self.write_raw_type(TY_STR_CHAR)?;
            self.buf.push(b[0]);
            Ok(())
        } else {
            // Always use TY_BUFFER: valid for any length, and skips the
            // (optional) string-table/sharing bookkeeping entirely, which
            // matches "skip shared references" on the encode side.
            self.write_raw_type(TY_BUFFER)?;
            self.write_buff(b);
            Ok(())
        }
    }

    fn write_str(&mut self, s: &str) -> Result<()> {
        if s.is_empty() {
            return self.write_raw_type(TY_UNICODE_0);
        }
        // TY_UTF8 is valid (and correctly decoded by Marshal.cpp) for any
        // non-empty unicode string, so there's no need to replicate the
        // UTF-16/UTF-8 length comparison the original writer does.
        self.write_raw_type(TY_UTF8)?;
        let bytes = s.as_bytes();
        self.write_integer(bytes.len() as i32);
        self.buf.extend_from_slice(bytes);
        Ok(())
    }

    fn write_tuple(&mut self, items: &[Value]) -> Result<()> {
        match items.len() {
            0 => self.write_raw_type(TY_TUPLE0),
            1 => {
                self.write_raw_type(TY_TUPLE1)?;
                self.write_value(&items[0])
            }
            2 => {
                self.write_raw_type(TY_TUPLE2)?;
                self.write_value(&items[0])?;
                self.write_value(&items[1])
            }
            n => {
                self.write_raw_type(TY_TUPLE)?;
                self.write_integer(n as i32);
                for it in items {
                    self.write_value(it)?;
                }
                Ok(())
            }
        }
    }

    fn write_list(&mut self, items: &[Value]) -> Result<()> {
        match items.len() {
            0 => self.write_raw_type(TY_LIST0),
            1 => {
                self.write_raw_type(TY_LIST1)?;
                self.write_value(&items[0])
            }
            n => {
                self.write_raw_type(TY_LIST)?;
                self.write_integer(n as i32);
                for it in items {
                    self.write_value(it)?;
                }
                Ok(())
            }
        }
    }

    fn write_dict(&mut self, pairs: &[(Value, Value)]) -> Result<()> {
        self.write_raw_type(TY_DICT)?;
        self.write_integer(pairs.len() as i32);
        for (k, v) in pairs {
            // NB: value before key, matching Marshal::WriteObject('d').
            self.write_value(v)?;
            self.write_value(k)?;
        }
        Ok(())
    }

    fn write_reduce(
        &mut self,
        newobj: bool,
        callable: &Value,
        args: &Value,
        state: Option<&Value>,
        list_items: &[Value],
        dict_items: &[(Value, Value)],
    ) -> Result<()> {
        self.write_raw_type(if newobj { TY_NEWOBJ } else { TY_REDUCE })?;
        let data = if newobj {
            let mut items = vec![args.clone()];
            if let Some(s) = state {
                items.push(s.clone());
            }
            Value::Tuple(items)
        } else {
            let mut items = vec![callable.clone(), args.clone()];
            if let Some(s) = state {
                items.push(s.clone());
            }
            Value::Tuple(items)
        };
        self.write_value(&data)?;
        for it in list_items {
            self.write_value(it)?;
        }
        self.write_raw_type(TY_MARK)?;
        for (k, v) in dict_items {
            self.write_value(k)?;
            self.write_value(v)?;
        }
        self.write_raw_type(TY_MARK)?;
        Ok(())
    }
}
