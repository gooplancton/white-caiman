use std::path::Path;

pub fn is_dir_empty(path: &Path) -> bool {
    path.read_dir()
        .map(|mut dir| dir.next().is_none())
        .unwrap_or(true)
}


