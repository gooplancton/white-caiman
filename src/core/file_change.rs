#![allow(deprecated)]

use std::{
    cmp::Ordering,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use anyhow::Context;
use bytes::Bytes;
use serde::Deserialize;
use watchman_client::prelude::*;

use super::{compression::compress_dir, message::FileChangeMessage, utils::is_dir_empty};

query_result_type! {
    pub struct FileChange {
        pub name: NameField,
        pub exists: ExistsField,
        pub is_new: NewField,
        pub ctime: CTimeField,
        pub mtime: MTimeField,
        pub typ: FileTypeField,
        pub ino: InodeNumberField,
    }
}

#[derive(Debug)]
pub struct SortedFileChanges {
    pub root_path: PathBuf,
    inner: Vec<FileChange>,
}

impl Deref for SortedFileChanges {
    type Target = Vec<FileChange>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for SortedFileChanges {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl SortedFileChanges {
    pub fn from(root_path: PathBuf, mut inner: Vec<FileChange>) -> Self {
        inner.sort_unstable_by(|change1, change2| {
            let ino1 = change1.ino.clone().into_inner();
            let ino2 = change2.ino.clone().into_inner();
            let ino_order = ino2.cmp(&ino1);
            if ino_order == Ordering::Equal {
                let ctime1 = change1.ctime.clone().into_inner();
                let ctime2 = change2.ctime.clone().into_inner();
                let ctime_order = ctime2.cmp(&ctime1);
                if ctime_order == Ordering::Equal {
                    let mtime1 = change1.mtime.clone().into_inner();
                    let mtime2 = change2.mtime.clone().into_inner();
                    mtime2.cmp(&mtime1)
                } else {
                    ctime_order
                }
            } else {
                ino_order
            }
        });

        Self { root_path, inner }
    }

    pub async fn next_message(&mut self) -> Option<FileChangeMessage> {
        let this_change = self.pop()?;
        let this_path = this_change.name.to_path_buf();
        let this_ino = this_change.ino.into_inner();

        let is_dir = matches!(this_change.typ.into_inner(), FileType::Directory);
        let is_new = this_change.is_new.into_inner();

        let exists = this_change.exists.into_inner();
        if exists {
            let message = match (is_dir, is_new) {
                (true, false) => FileChangeMessage::DirectoryContentsEdited(this_path),
                (false, true) => FileChangeMessage::FileCreated(this_path),
                (false, false) => {
                    let file_path = self.root_path.join(&this_path);
                    let contents = tokio::fs::read(file_path).await.unwrap(); // TODO: handle this
                    FileChangeMessage::FileEdited(this_path, Bytes::from(contents))
                }
                (true, true) => {
                    if is_dir_empty(this_path.as_path()) {
                        FileChangeMessage::EmptyDirectoryCreated(this_path)
                    } else {
                        let contents = compress_dir(this_path.as_path())
                            .await
                            .context("compressing dir")
                            .unwrap();

                        FileChangeMessage::DirectoryCreated(this_path, contents)
                    }
                }
            };

            return Some(message);
        }

        let next_ino_reached = !self
            .last()
            .is_some_and(|change| change.ino.abs_diff(this_ino) == 0);

        if next_ino_reached {
            let message = match is_dir {
                true => FileChangeMessage::DirectoryDeleted(this_path),
                false => FileChangeMessage::FileDeleted(this_path),
            };

            return Some(message);
        }

        let next_change = self.pop().unwrap();
        Some(FileChangeMessage::Rename(
            this_path,
            next_change.name.to_path_buf(),
        ))
    }
}

