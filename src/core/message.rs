use std::path::PathBuf;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

type OldPath = PathBuf;
type NewPath = PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub enum FileChangeMessage {
    FileCreated(PathBuf),
    FileDeleted(PathBuf),
    FileEdited(PathBuf, Bytes),
    EmptyDirectoryCreated(PathBuf),
    DirectoryCreated(PathBuf, Bytes),
    DirectoryDeleted(PathBuf),
    Rename(OldPath, NewPath),
    DirectoryContentsEdited(PathBuf),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RequestMessage {
    File(PathBuf),
    Dir(PathBuf)
}
