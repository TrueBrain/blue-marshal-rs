use crate::adler32::adler32;
use crate::error::{Error, Result};
use crate::tags::*;
use crate::value::Value;
use num_bigint::BigInt;

/// Result of decoding a top-level marshal buffer.
pub struct Decoded {
    pub value: Value,
    /// Whether the stream carried a `TY_CRC_CHECK` (matches
    /// `Marshal::mPacketHadCrc`).
    pub had_crc: bool,
    /// Number of bytes consumed from the input.
    pub consumed: usize,
}

/// Decode a single marshal buffer (starting with `TY_SIGNATURE`/`TY_SIGNATURE2`).
pub fn decode(buf: &[u8]) -> Result<Decoded> {
    let mut r = Reader::new(buf);
    r.read_header()?;
    let value = r.read_object()?;
    if r.got_crc && r.version > 0 {
        r.verify_crc(r.pos)?;
    }
    Ok(Decoded {
        value,
        had_crc: r.got_crc,
        consumed: r.pos,
    })
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    /// Usable content length: for version 0 this excludes the trailing
    /// shared-object index map, which lives at the very end of the buffer.
    ssize: usize,
    version: u8,
    // shared object table, resolved in place (see module docs in lib.rs).
    shared: Vec<Value>,
    // version 0 only: tail-of-buffer index map.
    mapping: Vec<i32>,
    num_shared_seen: usize,
    recursion: u32,
    got_crc: bool,
    crc_pos: usize,
    crc_val: i32,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader {
            buf,
            pos: 0,
            ssize: buf.len(),
            version: 0,
            shared: Vec::new(),
            mapping: Vec::new(),
            num_shared_seen: 0,
            recursion: 0,
            got_crc: false,
            crc_pos: 0,
            crc_val: 0,
        }
    }

    fn remaining(&self) -> usize {
        self.ssize.saturating_sub(self.pos)
    }

    fn check_space(&self, n: usize) -> Result<()> {
        if n > self.remaining() {
            return Err(Error::Eof);
        }
        Ok(())
    }

    fn read_u8(&mut self) -> Result<u8> {
        self.check_space(1)?;
        let b = self.buf[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.check_space(n)?;
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn read_i32(&mut self) -> Result<i32> {
        let b = self.read_bytes(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_i16(&mut self) -> Result<i16> {
        let b = self.read_bytes(2)?;
        Ok(i16::from_le_bytes([b[0], b[1]]))
    }

    fn read_i8(&mut self) -> Result<i8> {
        Ok(self.read_u8()? as i8)
    }

    fn read_i64(&mut self) -> Result<i64> {
        let b = self.read_bytes(8)?;
        Ok(i64::from_le_bytes(b.try_into().unwrap()))
    }

    fn read_f64(&mut self) -> Result<f64> {
        let b = self.read_bytes(8)?;
        Ok(f64::from_le_bytes(b.try_into().unwrap()))
    }

    /// Matches `ReadStream::ReadInteger`.
    fn read_integer(&mut self) -> Result<i32> {
        let c = self.read_u8()?;
        if c == u8::MAX {
            self.read_i32()
        } else {
            Ok(c as i32)
        }
    }

    /// Matches `ReadStream::GetBuff` (length-prefixed byte string).
    fn read_buff(&mut self) -> Result<&'a [u8]> {
        let len = self.read_integer()?;
        if len < 0 {
            return Err(Error::Invalid("negative buffer length".into()));
        }
        self.read_bytes(len as usize)
    }

    /// Matches `ReadStream::ReadType` (the raw byte, including shared flag).
    fn read_raw_type(&mut self) -> Result<u8> {
        self.read_u8()
    }

    fn peek_raw_type(&mut self) -> Result<u8> {
        let t = self.read_raw_type()?;
        self.pos -= 1;
        Ok(t)
    }

    fn read_header(&mut self) -> Result<()> {
        let tag = self.read_raw_type()?;
        if tag == TY_SIGNATURE {
            self.version = 0;
        } else if tag == TY_SIGNATURE2 {
            self.version = self.read_u8()?;
        } else {
            return Err(Error::Invalid("missing marshal signature".into()));
        }

        if self.version == 0 {
            let mapcount = self.read_i32()?;
            if mapcount < 0 {
                return Err(Error::Invalid("negative mapcount".into()));
            }
            if mapcount > 0 {
                let mapcount = mapcount as usize;
                let maplen = mapcount
                    .checked_mul(4)
                    .ok_or_else(|| Error::Invalid("mapcount overflow".into()))?;
                if maplen > self.buf.len() || self.remaining() < maplen {
                    return Err(Error::Invalid("truncated shared-object map".into()));
                }
                let map_start = self.buf.len() - maplen;
                let mut mapping = Vec::with_capacity(mapcount);
                for i in 0..mapcount {
                    let o = map_start + i * 4;
                    let v = i32::from_le_bytes([
                        self.buf[o],
                        self.buf[o + 1],
                        self.buf[o + 2],
                        self.buf[o + 3],
                    ]);
                    if v < 1 || v as usize > mapcount {
                        return Err(Error::Invalid("bogus shared-object map entry".into()));
                    }
                    mapping.push(v);
                }
                self.mapping = mapping;
                self.ssize = map_start;
                self.shared = vec![Value::None; mapcount];
            }
        }
        Ok(())
    }

    /// Allocate a shared slot for an object that is fully known up front
    /// (strings, globals, longs, ...): store it immediately.
    fn mark_and_store(&mut self, v: Value) -> Result<Value> {
        let ix = self.alloc_shared_slot()?;
        self.shared[ix] = v.clone();
        Ok(v)
    }

    /// Allocate a shared slot before the object's content is known
    /// (containers): fill in with `update_shared` once built.
    fn alloc_shared_slot(&mut self) -> Result<usize> {
        if self.version == 0 {
            if self.num_shared_seen >= self.mapping.len() {
                return Err(Error::Invalid("shared object table overflow".into()));
            }
            let ix = (self.mapping[self.num_shared_seen] - 1) as usize;
            self.num_shared_seen += 1;
            Ok(ix)
        } else {
            let ix = self.shared.len();
            self.shared.push(Value::None);
            Ok(ix)
        }
    }

    fn update_shared(&mut self, ix: usize, v: &Value) {
        self.shared[ix] = v.clone();
    }

    fn resolve_reference(&self, id: i32) -> Result<Value> {
        let ix = if self.version == 0 { id - 1 } else { id };
        if ix < 0 || ix as usize >= self.shared.len() {
            return Err(Error::BadReference(id as i64));
        }
        Ok(self.shared[ix as usize].clone())
    }

    fn verify_crc(&self, end: usize) -> Result<()> {
        let end = end.min(self.buf.len());
        if end < self.crc_pos {
            return Err(Error::Invalid("crc range invalid".into()));
        }
        let computed = adler32(1, &self.buf[self.crc_pos..end]) as i32;
        if computed != self.crc_val {
            return Err(Error::ChecksumMismatch);
        }
        Ok(())
    }

    /// Reads one object. `type & TY_SHAREDFLAG` marks the object as
    /// shared/referenceable, matching `Marshal::ReadObject`.
    fn read_object(&mut self) -> Result<Value> {
        self.recursion += 1;
        if self.recursion > RECURSION_LIMIT {
            return Err(Error::RecursionLimit);
        }
        let r = self.read_object_inner();
        self.recursion -= 1;
        r
    }

    fn read_object_inner(&mut self) -> Result<Value> {
        let raw = self.read_raw_type()?;
        let shared = (raw & TY_SHAREDFLAG) != 0;
        let ty = raw & TY_TYPEMASK;

        match ty {
            TY_NONE => Ok(Value::None),
            TY_TRUE => Ok(Value::Bool(true)),
            TY_FALSE => Ok(Value::Bool(false)),
            TY_INT_N1 => Ok(Value::Int(-1)),
            TY_INT_0 => Ok(Value::Int(0)),
            TY_INT_1 => Ok(Value::Int(1)),
            TY_INT8 => Ok(Value::Int(self.read_i8()? as i64)),
            TY_INT16 => Ok(Value::Int(self.read_i16()? as i64)),
            TY_INT32 => Ok(Value::Int(self.read_i32()? as i64)),
            TY_INT64 => Ok(Value::Int(self.read_i64()?)),
            TY_FLOAT_0 => Ok(Value::Float(0.0)),
            TY_FLOAT => Ok(Value::Float(self.read_f64()?)),
            TY_LONG => {
                let bytes = self.read_buff()?;
                let n = if bytes.is_empty() {
                    BigInt::from(0)
                } else {
                    BigInt::from_signed_bytes_le(bytes)
                };
                let v = Value::Long(n);
                if shared {
                    self.mark_and_store(v)
                } else {
                    Ok(v)
                }
            }
            TY_STR_EMPTY => Ok(Value::Bytes(Vec::new())),
            TY_STR_CHAR => Ok(Value::Bytes(vec![self.read_u8()?])),
            TY_STR_SHORT => {
                let n = self.read_u8()? as usize;
                Ok(Value::Bytes(self.read_bytes(n)?.to_vec()))
            }
            TY_STR_TABLE => {
                let idx = self.read_u8()?;
                let s = crate::strtable::lookup(idx)
                    .ok_or_else(|| Error::Invalid(format!("invalid string table index {idx}")))?;
                Ok(Value::Bytes(s.as_bytes().to_vec()))
            }
            TY_STR | TY_BUFFER => {
                let bytes = self.read_buff()?.to_vec();
                let v = Value::Bytes(bytes);
                if shared {
                    self.mark_and_store(v)
                } else {
                    Ok(v)
                }
            }
            TY_UNICODE_0 => Ok(Value::Str(String::new())),
            TY_UNICODE_1 => {
                let b = self.read_bytes(2)?;
                let code = u16::from_le_bytes([b[0], b[1]]);
                let s = String::from_utf16(&[code])
                    .map_err(|_| Error::Invalid("bad utf-16 code unit".into()))?;
                Ok(Value::Str(s))
            }
            TY_UNICODE => {
                let len = self.read_integer()?;
                if len < 0 {
                    return Err(Error::Invalid("negative unicode length".into()));
                }
                let byte_len = (len as usize)
                    .checked_mul(2)
                    .ok_or_else(|| Error::Invalid("unicode length overflow".into()))?;
                let bytes = self.read_bytes(byte_len)?;
                let units: Vec<u16> = bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let s = String::from_utf16(&units)
                    .map_err(|_| Error::Invalid("bad utf-16 data".into()))?;
                Ok(Value::Str(s))
            }
            TY_UTF8 => {
                let len = self.read_integer()?;
                if len < 0 {
                    return Err(Error::Invalid("negative utf8 length".into()));
                }
                let bytes = self.read_bytes(len as usize)?;
                let s = String::from_utf8(bytes.to_vec())
                    .map_err(|_| Error::Invalid("bad utf-8 data".into()))?;
                Ok(Value::Str(s))
            }
            TY_GLOBAL => {
                let name = self.read_buff()?;
                let name = String::from_utf8(name.to_vec())
                    .map_err(|_| Error::Invalid("bad utf-8 in global name".into()))?;
                let v = Value::Global(name);
                if shared {
                    self.mark_and_store(v)
                } else {
                    Ok(v)
                }
            }
            TY_TUPLE0 => Ok(Value::Tuple(Vec::new())),
            TY_TUPLE1 => {
                let ix = if shared {
                    Some(self.alloc_shared_slot()?)
                } else {
                    None
                };
                let item = self.read_object()?;
                let v = Value::Tuple(vec![item]);
                if let Some(ix) = ix {
                    self.update_shared(ix, &v);
                }
                Ok(v)
            }
            TY_TUPLE2 => {
                let ix = if shared {
                    Some(self.alloc_shared_slot()?)
                } else {
                    None
                };
                let a = self.read_object()?;
                let b = self.read_object()?;
                let v = Value::Tuple(vec![a, b]);
                if let Some(ix) = ix {
                    self.update_shared(ix, &v);
                }
                Ok(v)
            }
            TY_TUPLE => {
                let len = self.read_integer()?;
                if len < 0 {
                    return Err(Error::Invalid("negative tuple length".into()));
                }
                self.check_space(len as usize)?;
                let ix = if shared {
                    Some(self.alloc_shared_slot()?)
                } else {
                    None
                };
                let mut items = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    items.push(self.read_object()?);
                }
                let v = Value::Tuple(items);
                if let Some(ix) = ix {
                    self.update_shared(ix, &v);
                }
                Ok(v)
            }
            TY_LIST0 => Ok(Value::List(Vec::new())),
            TY_LIST1 => {
                let ix = if shared {
                    Some(self.alloc_shared_slot()?)
                } else {
                    None
                };
                let item = self.read_object()?;
                let v = Value::List(vec![item]);
                if let Some(ix) = ix {
                    self.update_shared(ix, &v);
                }
                Ok(v)
            }
            TY_LIST => {
                let len = self.read_integer()?;
                if len < 0 {
                    return Err(Error::Invalid("negative list length".into()));
                }
                self.check_space(len as usize)?;
                let ix = if shared {
                    Some(self.alloc_shared_slot()?)
                } else {
                    None
                };
                let mut items = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    items.push(self.read_object()?);
                }
                let v = Value::List(items);
                if let Some(ix) = ix {
                    self.update_shared(ix, &v);
                }
                Ok(v)
            }
            TY_DICT => {
                let len = self.read_integer()?;
                if len < 0 {
                    return Err(Error::Invalid("negative dict length".into()));
                }
                // Each pair is at least two 1-byte objects.
                self.check_space((len as usize).saturating_mul(2))?;
                let ix = if shared {
                    Some(self.alloc_shared_slot()?)
                } else {
                    None
                };
                let mut pairs = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    // NB: value is written/read before key, see WriteObject('d').
                    let val = self.read_object()?;
                    let key = self.read_object()?;
                    pairs.push((key, val));
                }
                let v = Value::Dict(pairs);
                if let Some(ix) = ix {
                    self.update_shared(ix, &v);
                }
                Ok(v)
            }
            TY_INSTANCE => {
                let ix = if shared {
                    Some(self.alloc_shared_slot()?)
                } else {
                    None
                };
                let guid = self.read_object()?;
                let class = guid
                    .as_str_like()
                    .ok_or_else(|| Error::Invalid("TY_INSTANCE guid isn't a string".into()))?;
                let data = self.read_object()?;
                let v = Value::Instance {
                    class,
                    state: Box::new(data),
                };
                if let Some(ix) = ix {
                    self.update_shared(ix, &v);
                }
                Ok(v)
            }
            TY_REDUCE | TY_NEWOBJ => {
                let newobj = ty == TY_NEWOBJ;
                let ix = if shared {
                    Some(self.alloc_shared_slot()?)
                } else {
                    None
                };
                let rv = self.read_object()?;
                let items = match rv {
                    Value::Tuple(items) => items,
                    _ => return Err(Error::Invalid("reduce payload isn't a tuple".into())),
                };
                let (callable, args, state) = if newobj {
                    if items.is_empty() {
                        return Err(Error::Invalid("empty newobj tuple".into()));
                    }
                    let args = items[0].clone();
                    let state = items.get(1).cloned();
                    (Value::None, args, state)
                } else {
                    if items.len() < 2 {
                        return Err(Error::Invalid("reduce tuple too short".into()));
                    }
                    (items[0].clone(), items[1].clone(), items.get(2).cloned())
                };
                let list_items = self.read_list_iter()?;
                let dict_items = self.read_dict_iter()?;
                let v = Value::Reduce {
                    newobj,
                    callable: Box::new(callable),
                    args: Box::new(args),
                    state: state.map(Box::new),
                    list_items,
                    dict_items,
                };
                if let Some(ix) = ix {
                    self.update_shared(ix, &v);
                }
                Ok(v)
            }
            TY_CALLBACK => {
                let data = self.read_object()?;
                Ok(Value::Callback(Box::new(data)))
            }
            TY_REFERENCE => {
                let id = self.read_integer()?;
                self.resolve_reference(id)
            }
            TY_CRC_CHECK => {
                let crc = self.read_i32()?;
                self.got_crc = true;
                self.crc_val = crc;
                self.crc_pos = self.pos;
                if self.version == 0 {
                    self.verify_crc(self.buf.len())?;
                }
                self.read_object()
            }
            TY_DBROW => Err(Error::Unsupported("TY_DBROW")),
            TY_WSTREAM => Err(Error::Unsupported("TY_WSTREAM")),
            TY_PICKLE => Err(Error::Unsupported("TY_PICKLE")),
            TY_PICKLER => Err(Error::Unsupported("TY_PICKLER")),
            TY_COMPLEX => Err(Error::Unsupported("TY_COMPLEX")),
            other => Err(Error::Invalid(format!(
                "unknown type tag {other} ({})",
                tag_name(other)
            ))),
        }
    }

    /// Matches `ReadObjectListIter`: read values until a `TY_MARK`.
    fn read_list_iter(&mut self) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        loop {
            let t = self.peek_raw_type()?;
            if (t & TY_TYPEMASK) == TY_MARK {
                self.read_raw_type()?;
                return Ok(out);
            }
            out.push(self.read_object()?);
        }
    }

    /// Matches `ReadObjectDictIter`: read (key, value) pairs until a `TY_MARK`.
    fn read_dict_iter(&mut self) -> Result<Vec<(Value, Value)>> {
        let mut out = Vec::new();
        loop {
            let t = self.peek_raw_type()?;
            if (t & TY_TYPEMASK) == TY_MARK {
                self.read_raw_type()?;
                return Ok(out);
            }
            let key = self.read_object()?;
            let val = self.read_object()?;
            out.push((key, val));
        }
    }
}
