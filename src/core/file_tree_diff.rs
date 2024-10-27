use std::{
    collections::BTreeMap,
    fmt::Display,
    path::{Path, PathBuf},
};

use super::{
    file_tree::{FileTree, FileTreeNodeType},
    message::RequestMessage,
};

#[derive(Debug)]
pub struct TreeDiff<'message> {
    created_dirs: Vec<&'message Path>,
    deleted_dirs: Vec<&'message Path>,
    created_files: Vec<&'message Path>,
    deleted_files: Vec<&'message Path>,
    edited_files: Vec<&'message Path>,
}

impl Display for TreeDiff<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Symbols for different states
        const DELETED: &str = "✗"; // Red X for deleted
        const CREATED: &str = "+"; // Plus for created
        const EDITED: &str = "✎"; // Pencil for edited
        const TREE_BRANCH: &str = "├── ";
        const TREE_CORNER: &str = "└── ";
        const TREE_VERTICAL: &str = "│   ";
        const TREE_SPACE: &str = "    ";

        // Create a tree structure
        let mut tree: BTreeMap<PathBuf, (String, bool)> = BTreeMap::new(); // (symbol, is_dir)

        // Helper to get parent paths
        let get_parents = |path: &Path| {
            let mut parents = vec![];
            let mut current = path;
            while let Some(parent) = current.parent() {
                if !parent.as_os_str().is_empty() {
                    parents.push(parent.to_path_buf());
                }
                current = parent;
            }
            parents
        };

        // Add all paths to the tree with their symbols
        for &path in self.deleted_dirs.iter() {
            for parent in get_parents(path) {
                tree.entry(parent).or_insert_with(|| ("".to_string(), true));
            }
            tree.insert(path.to_path_buf(), (DELETED.to_string(), true));
        }

        for &path in self.deleted_files.iter() {
            for parent in get_parents(path) {
                tree.entry(parent).or_insert_with(|| ("".to_string(), true));
            }
            tree.insert(path.to_path_buf(), (DELETED.to_string(), false));
        }

        for &path in self.created_dirs.iter() {
            for parent in get_parents(path) {
                tree.entry(parent).or_insert_with(|| ("".to_string(), true));
            }
            tree.insert(path.to_path_buf(), (CREATED.to_string(), true));
        }

        for &path in self.created_files.iter() {
            for parent in get_parents(path) {
                tree.entry(parent).or_insert_with(|| ("".to_string(), true));
            }
            tree.insert(path.to_path_buf(), (CREATED.to_string(), false));
        }

        for &path in self.edited_files.iter() {
            for parent in get_parents(path) {
                tree.entry(parent).or_insert_with(|| ("".to_string(), true));
            }
            tree.insert(path.to_path_buf(), (EDITED.to_string(), false));
        }

        // Helper function to recursively print the tree
        fn print_tree(
            f: &mut std::fmt::Formatter<'_>,
            tree: &BTreeMap<PathBuf, (String, bool)>,
            current_path: &Path,
            prefix: &str,
            is_last: bool,
        ) -> std::fmt::Result {
            let node = &tree.get(current_path);
            let is_dir = node.as_ref().map(|node| node.1).unwrap_or(false);

            // Print current node
            if !current_path.as_os_str().is_empty() {
                let connector = if is_last { TREE_CORNER } else { TREE_BRANCH };
                writeln!(
                    f,
                    "{}{}{} {}{}",
                    prefix,
                    connector,
                    node.as_ref()
                        .map(|node| node.0.as_str())
                        .unwrap_or_default(),
                    current_path.file_name().unwrap().to_str().unwrap(),
                    if is_dir { "/" } else { "" }
                )?;
            }

            // Get children of current path
            let children: Vec<_> = tree
                .keys()
                .filter(|p| p.parent() == Some(current_path))
                .collect();

            // Recursively print children
            for (i, child) in children.iter().enumerate() {
                let is_last_child = i == children.len() - 1;
                let new_prefix = if current_path.as_os_str().is_empty() {
                    "".to_string()
                } else if is_last {
                    format!("{}{}", prefix, TREE_SPACE)
                } else {
                    format!("{}{}", prefix, TREE_VERTICAL)
                };

                print_tree(f, tree, child, &new_prefix, is_last_child)?;
            }

            Ok(())
        }

        // Print the complete tree starting from root
        writeln!(f, ".")?;
        print_tree(f, &tree, Path::new(""), "", true)?;

        Ok(())
    }
}

impl<'tree> TreeDiff<'tree> {
    pub fn from(local_tree: &'tree FileTree, remote_tree: &'tree FileTree) -> Self {
        let (mut local_idx, mut remote_idx) = (0, 0);
        let mut diff = Self {
            created_dirs: vec![],
            deleted_dirs: vec![],
            created_files: vec![],
            deleted_files: vec![],
            edited_files: vec![],
        };

        while local_idx < local_tree.len() && remote_idx < remote_tree.len() {
            let local_node = local_tree.get(local_idx).unwrap();
            let remote_node = remote_tree.get(remote_idx).unwrap();

            match (&local_node.typ, &remote_node.typ) {
                (
                    FileTreeNodeType::File { sha1: local_sha },
                    FileTreeNodeType::File { sha1: remote_sha },
                ) => match local_node.path.cmp(&remote_node.path) {
                    std::cmp::Ordering::Greater => {
                        diff.created_files.push(&remote_node.path);
                        remote_idx += 1;
                    }
                    std::cmp::Ordering::Less => {
                        diff.deleted_files.push(&local_node.path);
                        local_idx += 1;
                    }
                    std::cmp::Ordering::Equal => {
                        if local_sha != remote_sha {
                            diff.edited_files.push(&local_node.path)
                        }

                        local_idx += 1;
                        remote_idx += 1;
                    }
                },
                (FileTreeNodeType::File { sha1: _ }, FileTreeNodeType::Dir) => {
                    diff.deleted_files.push(&local_node.path);
                    local_idx += 1;
                }
                (FileTreeNodeType::Dir, FileTreeNodeType::File { sha1: _ }) => {
                    diff.created_files.push(&remote_node.path);
                    remote_idx += 1;
                }
                (FileTreeNodeType::Dir, FileTreeNodeType::Dir) => {
                    match local_node.path.cmp(&remote_node.path) {
                        std::cmp::Ordering::Less => {
                            diff.deleted_dirs.push(&local_node.path);
                            let local_idx_offset = local_tree
                                .get(local_idx..)
                                .unwrap()
                                .iter()
                                .position(|node| !node.path.starts_with(&local_node.path))
                                .unwrap_or(local_tree.len() - local_idx);
                            local_idx += local_idx_offset;
                        }
                        std::cmp::Ordering::Greater => {
                            diff.created_dirs.push(&remote_node.path);
                            let remote_idx_offset = remote_tree
                                .get(remote_idx..)
                                .unwrap()
                                .iter()
                                .position(|node| !node.path.starts_with(&remote_node.path))
                                .unwrap_or(remote_tree.len() - remote_idx);
                            remote_idx += remote_idx_offset;
                        }
                        std::cmp::Ordering::Equal => {
                            local_idx += 1;
                            remote_idx += 1;
                        }
                    }
                }
            }
        }

        while let Some(node) = local_tree.get(local_idx) {
            match &node.typ {
                FileTreeNodeType::File { sha1: _ } => {
                    diff.deleted_files.push(&node.path);
                    local_idx += 1;
                }
                FileTreeNodeType::Dir => {
                    diff.deleted_dirs.push(&node.path);
                    let local_idx_offset = local_tree
                        .get(local_idx..)
                        .unwrap()
                        .iter()
                        .position(|node| !node.path.starts_with(&node.path))
                        .unwrap_or(local_tree.len() - local_idx);
                    local_idx += local_idx_offset;
                }
            }
        }

        while let Some(node) = remote_tree.get(remote_idx) {
            match &node.typ {
                FileTreeNodeType::File { sha1: _ } => {
                    diff.created_files.push(&node.path);
                    remote_idx += 1;
                }
                FileTreeNodeType::Dir => {
                    diff.created_dirs.push(&node.path);
                    let remote_idx_offset = remote_tree
                        .get(remote_idx..)
                        .unwrap()
                        .iter()
                        .position(|node| !node.path.starts_with(&node.path))
                        .unwrap_or(remote_tree.len() - remote_idx);
                    remote_idx += remote_idx_offset;
                }
            }
        }

        diff
    }

    pub async fn apply(&self, root_path: &Path) -> Vec<RequestMessage> {
        for deleted_dir in self.deleted_dirs.iter() {
            let path = root_path.join(deleted_dir);
            let _ = tokio::fs::remove_dir_all(path).await;
        }

        for deleted_file in self.deleted_files.iter() {
            let path = root_path.join(deleted_file);
            let _ = tokio::fs::remove_file(path).await;
        }

        let mut requests = Vec::<RequestMessage>::with_capacity(
            self.created_dirs.len() + self.created_files.len() + self.edited_files.len(),
        );

        for &path in self.created_dirs.iter() {
            requests.push(RequestMessage::Dir(path.to_owned()))
        }

        for &path in self.created_files.iter() {
            requests.push(RequestMessage::File(path.to_owned()))
        }

        for &path in self.edited_files.iter() {
            requests.push(RequestMessage::File(path.to_owned()))
        }

        requests
    }
}
