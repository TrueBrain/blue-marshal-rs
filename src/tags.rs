//! Wire type tags, mirroring `enum PYTYPES` in Marshal.h exactly.
// Kept 1:1 with the C++ enum for reference even where a given tag is only
// ever matched in one direction (e.g. the TY_DBROW/TY_WSTREAM/TY_PICKLE*
// family, which reader.rs rejects via Error::Unsupported and the writer
// never emits) - hence the blanket allow rather than per-constant ones.
#![allow(dead_code)]

/// Max nesting depth for reading/writing a `Value` tree. Shared by both
/// reader and writer so the two limits can't drift apart.
pub const RECURSION_LIMIT: u32 = 1000;

pub const TY_INVALID: u8 = 0;
pub const TY_SIGNATURE: u8 = 126;
pub const TY_SIGNATURE2: u8 = 125;

pub const TY_NONE: u8 = 1;
pub const TY_GLOBAL: u8 = 2;

pub const TY_INT64: u8 = 3;
pub const TY_INT32: u8 = 4;
pub const TY_INT16: u8 = 5;
pub const TY_INT8: u8 = 6;
pub const TY_INT_N1: u8 = 7;
pub const TY_INT_0: u8 = 8;
pub const TY_INT_1: u8 = 9;

pub const TY_FLOAT: u8 = 10;
pub const TY_FLOAT_0: u8 = 11;

pub const TY_COMPLEX: u8 = 12;

pub const TY_STR: u8 = 13;
pub const TY_STR_EMPTY: u8 = 14;
pub const TY_STR_CHAR: u8 = 15;
pub const TY_STR_SHORT: u8 = 16;
pub const TY_STR_TABLE: u8 = 17;
pub const TY_UNICODE: u8 = 18;

pub const TY_BUFFER: u8 = 19;
pub const TY_TUPLE: u8 = 20;
pub const TY_LIST: u8 = 21;

pub const TY_DICT: u8 = 22;

pub const TY_INSTANCE: u8 = 23;
pub const TY_CALLBACK: u8 = 25;

pub const TY_PICKLE: u8 = 26;

pub const TY_REFERENCE: u8 = 27;

pub const TY_CRC_CHECK: u8 = 28;

pub const TY_TRUE: u8 = 31;
pub const TY_FALSE: u8 = 32;

pub const TY_PICKLER: u8 = 33;
pub const TY_REDUCE: u8 = 34;
pub const TY_NEWOBJ: u8 = 35;

pub const TY_TUPLE0: u8 = 36;
pub const TY_TUPLE1: u8 = 37;
pub const TY_LIST0: u8 = 38;
pub const TY_LIST1: u8 = 39;

pub const TY_UNICODE_0: u8 = 40;
pub const TY_UNICODE_1: u8 = 41;

pub const TY_DBROW: u8 = 42;
pub const TY_WSTREAM: u8 = 43;

pub const TY_TUPLE2: u8 = 44;
pub const TY_MARK: u8 = 45;

pub const TY_UTF8: u8 = 46;

pub const TY_LONG: u8 = 47;

pub const TY_SHAREDFLAG: u8 = 0x40;
pub const TY_TYPEMASK: u8 = 0x3F;

/// Human readable name for a (masked) type tag, used in error messages.
pub fn tag_name(t: u8) -> &'static str {
    match t {
        TY_NONE => "NONE",
        TY_GLOBAL => "GLOBAL",
        TY_INT64 => "INT64",
        TY_INT32 => "INT32",
        TY_INT16 => "INT16",
        TY_INT8 => "INT8",
        TY_INT_N1 => "INT_N1",
        TY_INT_0 => "INT_0",
        TY_INT_1 => "INT_1",
        TY_FLOAT => "FLOAT",
        TY_FLOAT_0 => "FLOAT_0",
        TY_COMPLEX => "COMPLEX",
        TY_STR => "STR",
        TY_STR_EMPTY => "STR_EMPTY",
        TY_STR_CHAR => "STR_CHAR",
        TY_STR_SHORT => "STR_SHORT",
        TY_STR_TABLE => "STR_TABLE",
        TY_UNICODE => "UNICODE",
        TY_BUFFER => "BUFFER",
        TY_TUPLE => "TUPLE",
        TY_LIST => "LIST",
        TY_DICT => "DICT",
        TY_INSTANCE => "INSTANCE",
        TY_CALLBACK => "CALLBACK",
        TY_PICKLE => "PICKLE",
        TY_REFERENCE => "REFERENCE",
        TY_CRC_CHECK => "CRC_CHECK",
        TY_TRUE => "TRUE",
        TY_FALSE => "FALSE",
        TY_PICKLER => "PICKLER",
        TY_REDUCE => "REDUCE",
        TY_NEWOBJ => "NEWOBJ",
        TY_TUPLE0 => "TUPLE0",
        TY_TUPLE1 => "TUPLE1",
        TY_LIST0 => "LIST0",
        TY_LIST1 => "LIST1",
        TY_UNICODE_0 => "UNICODE_0",
        TY_UNICODE_1 => "UNICODE_1",
        TY_DBROW => "DBROW",
        TY_WSTREAM => "WSTREAM",
        TY_TUPLE2 => "TUPLE2",
        TY_MARK => "MARK",
        TY_UTF8 => "UTF8",
        TY_LONG => "LONG",
        _ => "UNKNOWN",
    }
}
