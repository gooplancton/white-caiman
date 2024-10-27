use anyhow::bail;
use sha1::{Digest, Sha1};
use std::{
    fs,
    ops::Deref,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct FileTreeNode {
    pub path: PathBuf,
    pub typ: FileTreeNodeType,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum FileTreeNodeType {
    File { sha1: [u8; 20] },
    Dir,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileTree {
    nodes: Vec<FileTreeNode>,
}

impl Deref for FileTree {
    type Target = Vec<FileTreeNode>;

    fn deref(&self) -> &Self::Target {
        self.nodes.as_ref()
    }
}

impl FileTree {
    pub async fn new(base_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let base_path = base_path.as_ref();
        if !base_path.try_exists().is_ok_and(|exists| exists) {
            fs::create_dir(base_path)?;
        }

        if !base_path.is_dir() {
            bail!("provided path is not a directory")
        }

        let mut nodes = vec![];

        let mut handles = vec![];
        for entry in WalkDir::new(base_path)
            .sort_by(|entry1, entry2| entry1.path().cmp(entry2.path()))
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let meta = entry.metadata();
            if meta.is_err() {
                continue;
            }

            let is_file = meta.unwrap().is_file();

            if is_file {
                let full_path = entry.path().to_owned();
                let truncated_path = entry.path().strip_prefix(base_path).unwrap().to_owned();
                handles.push(tokio::spawn(async {
                    let file = tokio::fs::read(full_path).await.unwrap();

                    let mut hasher = Sha1::new();
                    hasher.update(&file);
                    let sha1: [u8; 20] = hasher.finalize().into();

                    FileTreeNode {
                        path: truncated_path,
                        typ: FileTreeNodeType::File { sha1 },
                    }
                }));
            } else {
                let path = entry.path().strip_prefix(base_path).unwrap().to_owned();
                handles.push(tokio::spawn(async {
                    FileTreeNode {
                        path,
                        typ: FileTreeNodeType::Dir,
                    }
                }))
            }
        }

        nodes.reserve(handles.len());
        for handle in handles {
            nodes.push(handle.await.unwrap());
        }

        Ok(Self { nodes })
    }

    pub fn is_valid(&self) -> bool {
        for (i, node) in self.nodes.iter().enumerate() {
            if self
                .nodes
                .get(i + 1)
                .is_some_and(|next_node| node.path > next_node.path)
            {
                return false;
            }
        }

        true
    }
}
