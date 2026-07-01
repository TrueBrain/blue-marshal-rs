use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// Ran past the end of the input buffer.
    Eof,
    /// A structurally invalid stream (bad header, bad tag, bad lengths, ...).
    Invalid(String),
    /// A `TY_REFERENCE` pointed at a slot that doesn't exist (or isn't
    /// filled in yet).
    BadReference(i64),
    /// Adler32 checksum stored in the stream didn't match the computed one.
    ChecksumMismatch,
    /// A wire feature this port intentionally doesn't implement
    /// (TY_DBROW, TY_WSTREAM, TY_PICKLE, TY_PICKLER).
    Unsupported(&'static str),
    /// Recursion depth guard, mirrors `Marshal::sRecursionLimit`.
    RecursionLimit,
    /// JSON round-trip specific errors (bad prefix, malformed compound key...).
    Json(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Eof => write!(f, "unexpected end of stream"),
            Error::Invalid(s) => write!(f, "invalid marshal stream: {s}"),
            Error::BadReference(id) => write!(f, "invalid TY_REFERENCE id {id}"),
            Error::ChecksumMismatch => write!(f, "adler32 checksum mismatch"),
            Error::Unsupported(what) => write!(f, "unsupported marshal feature: {what}"),
            Error::RecursionLimit => write!(f, "maximum recursion depth reached"),
            Error::Json(s) => write!(f, "invalid JSON encoding: {s}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
