#![allow(dead_code)]

use anyhow::bail;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
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
    n_dirs: usize,
    nodes: Vec<FileTreeNode>,
}

impl FileTree {
    pub fn dirs(&self) -> &[FileTreeNode] {
        self.nodes.get(..self.n_dirs).unwrap()
    }

    pub fn files(&self) -> &[FileTreeNode] {
        self.nodes.get(self.n_dirs..).unwrap()
    }

    pub async fn new(base_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let base_path = base_path.as_ref();
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
                nodes.push(FileTreeNode {
                    path,
                    typ: FileTreeNodeType::Dir,
                })
            }
        }

        let n_dirs = nodes.len();
        nodes.reserve(handles.len());
        for handle in handles {
            nodes.push(handle.await.unwrap());
        }

        Ok(Self { n_dirs, nodes })
    }
}

#[derive(Debug)]
pub struct TreeDiff<'remote> {
    created_dirs: Vec<&'remote Path>,
    deleted_dirs: Vec<&'remote Path>,
    created_files: Vec<&'remote Path>,
    deleted_files: Vec<&'remote Path>,
    edited_files: Vec<&'remote Path>,
}

impl<'remote> TreeDiff<'remote> {
    pub fn from<'local>(local_tree: &'local FileTree, remote_tree: &'remote FileTree) -> Self
    where
        'local: 'remote,
    {
        let local_dirs = local_tree.dirs();
        let remote_dirs = remote_tree.dirs();
        let (created_dirs, deleted_dirs, _) = diff(local_dirs, remote_dirs);

        let local_files = local_tree.files();
        let remote_files = remote_tree.files();
        let (created_files, deleted_files, edited_files) = diff(local_files, remote_files);

        Self {
            created_dirs,
            deleted_dirs,
            created_files,
            deleted_files,
            edited_files,
        }
    }

    pub async fn apply(&self, root_path: &Path) -> anyhow::Result<Vec<PathBuf>> {
        for deleted_dir in self.deleted_dirs.iter() {
            let path = root_path.join(deleted_dir);
            let _ = tokio::fs::remove_dir_all(path).await;
        }

        for created_dir in self.created_dirs.iter() {
            let path = root_path.join(created_dir);
            let _ = tokio::fs::remove_dir_all(path).await;
        }

        for deleted_file in self.deleted_files.iter() {
            let path = root_path.join(deleted_file);
            let _ = tokio::fs::remove_file(path).await;
        }

        let mut requested_files =
            Vec::<PathBuf>::with_capacity(self.created_files.len() + self.edited_files.len());

        for &path in self.created_files.iter() {
            requested_files.push(path.to_owned())
        }

        for &path in self.edited_files.iter() {
            requested_files.push(path.to_owned())
        }

        Ok(requested_files)
    }
}

fn diff<'remote, 'local: 'remote>(
    local_nodes: &'local [FileTreeNode],
    remote_nodes: &'remote [FileTreeNode],
) -> (Vec<&'remote Path>, Vec<&'remote Path>, Vec<&'remote Path>) {
    let (mut local_idx, mut remote_idx) = (0, 0);
    let (mut created_paths, mut deleted_paths, mut edited_paths) = (vec![], vec![], vec![]);

    let (local_len, remote_len) = (local_nodes.len(), remote_nodes.len());
    while local_idx < local_len && remote_idx < remote_len {
        let local_node = local_nodes.get(local_idx).unwrap();
        let remote_node = remote_nodes.get(remote_idx).unwrap();

        match local_node.path.cmp(&remote_node.path) {
            std::cmp::Ordering::Equal => {
                if let FileTreeNodeType::File { sha1: local_sha1 } = local_node.typ {
                    if let FileTreeNodeType::File { sha1: remote_sha1 } = remote_node.typ {
                        if local_sha1 != remote_sha1 {
                            edited_paths.push(local_node.path.as_path());
                        }
                    } else {
                        panic!("trying to compare file with directory")
                    }
                }

                local_idx += 1;
                remote_idx += 1;
            }
            std::cmp::Ordering::Less => {
                deleted_paths.push(local_node.path.as_path());
                local_idx += 1;
            }
            std::cmp::Ordering::Greater => {
                created_paths.push(remote_node.path.as_path());
                remote_idx += 1;
            }
        }
    }

    if local_idx < local_len {
        for node in local_nodes.get(local_idx..).unwrap() {
            deleted_paths.push(node.path.as_path());
        }
    } else if remote_idx < remote_len {
        for node in remote_nodes.get(remote_idx..).unwrap() {
            created_paths.push(node.path.as_path());
        }
    }

    (created_paths, deleted_paths, edited_paths)
}
