// SPDX-License-Identifier: GPL-3.0-or-later
//
// Single unified error enum. Every module reports through this `Error`.
// Serialized when returned from Tauri commands; the frontend receives
// `{ kind, message }` and dispatches on the typed `kind` discriminant.

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Db(String),

    #[error("Volume error: {0}")]
    Volume(String),

    #[error("Scan error: {0}")]
    Scan(String),

    #[error("Staging error: {0}")]
    Staging(String),

    #[error("Snapshot error: {0}")]
    Snapshot(String),

    #[error("Duplicate detector error: {0}")]
    Duplicate(String),

    #[error("Locked file: {0}")]
    LockedFile(String),

    #[error("USN index error: {0}")]
    Index(String),

    #[error("Not implemented yet: {0}")]
    NotImplemented(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Error", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

impl Error {
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Io(_) => "io",
            Error::Db(_) => "db",
            Error::Volume(_) => "volume",
            Error::Scan(_) => "scan",
            Error::Staging(_) => "staging",
            Error::Snapshot(_) => "snapshot",
            Error::Duplicate(_) => "duplicate",
            Error::LockedFile(_) => "locked_file",
            Error::Index(_) => "index",
            Error::NotImplemented(_) => "not_implemented",
        }
    }
}
