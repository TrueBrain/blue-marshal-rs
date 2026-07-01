use num_bigint::BigInt;

/// A decoded blue.Marshal object.
///
/// This mirrors the set of things `Marshal.cpp` can produce, minus a few
/// forms that are intentionally unsupported by this port (see `README.md`):
/// `TY_DBROW`, `TY_WSTREAM`, `TY_PICKLE` and `TY_PICKLER`. Streams containing
/// those tags will fail to decode with `Error::Unsupported`.
///
/// `Int` and `Long` are kept distinct (mirroring Python 2's `int` vs `long`)
/// so re-encoding chooses the same wire representation family
/// (`TY_INT*` vs `TY_LONG`) the original data used.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    Long(BigInt),
    Float(f64),
    /// A Python 2 byte string (`str` / `buffer`).
    Bytes(Vec<u8>),
    /// A Python unicode string.
    Str(String),
    Tuple(Vec<Value>),
    List(Vec<Value>),
    /// Order-preserving; Python dicts have no guaranteed order but we keep
    /// insertion/stream order so round-tripping is stable.
    Dict(Vec<(Value, Value)>),
    /// `TY_GLOBAL`: a class/type/function referenced by its fully qualified
    /// `module.name` string.
    Global(String),
    /// `TY_INSTANCE`: an old-style class instance. `class` is the
    /// `module.ClassName` guid string, `state` is either the `__getstate__()`
    /// result or the instance `__dict__`.
    Instance {
        class: String,
        state: Box<Value>,
    },
    /// `TY_REDUCE` / `TY_NEWOBJ`: the `__reduce__` pickle protocol.
    Reduce {
        /// true => was TY_NEWOBJ (callable is implicit `__newobj__`, so it's
        /// not stored - `callable` is `None` in that case), false => TY_REDUCE.
        newobj: bool,
        callable: Box<Value>,
        args: Box<Value>,
        state: Option<Box<Value>>,
        list_items: Vec<Value>,
        dict_items: Vec<(Value, Value)>,
    },
    /// `TY_CALLBACK`: data that was routed through a save/load callback.
    /// We don't invoke anything, we just preserve the wrapped payload.
    Callback(Box<Value>),
}

impl Value {
    /// Returns the text content of `Bytes` (lossily) or `Str`, or `None` for
    /// any other variant. Used when decoding `TY_INSTANCE` to coerce its
    /// guid field into a class name string.
    pub fn as_str_like(&self) -> Option<String> {
        match self {
            Value::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
            Value::Str(s) => Some(s.clone()),
            _ => None,
        }
    }
}
