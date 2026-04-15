use std::collections::HashMap;
use std::path::Path;

use crate::models::{FileInfo, FileType};

pub(crate) struct DirectoryTree {
    child_dirs: HashMap<String, Vec<String>>,
    dirs_deepest_first: Vec<String>,
}

impl DirectoryTree {
    pub(crate) fn build(files: &[FileInfo]) -> Self {
        let mut child_dirs: HashMap<String, Vec<String>> = HashMap::new();
        let mut dir_paths: Vec<String> = Vec::new();

        for entry in files {
            if entry.file_type == FileType::Directory {
                dir_paths.push(entry.path.clone());
            }
            if let Some(parent) = Path::new(&entry.path).parent().and_then(|p| p.to_str())
                && !parent.is_empty()
                && entry.file_type == FileType::Directory
            {
                child_dirs
                    .entry(parent.to_string())
                    .or_default()
                    .push(entry.path.clone());
            }
        }

        dir_paths.sort_by_key(|path| usize::MAX - Path::new(path).components().count());

        DirectoryTree {
            child_dirs,
            dirs_deepest_first: dir_paths,
        }
    }

    pub(crate) fn dirs_deepest_first(&self) -> &[String] {
        &self.dirs_deepest_first
    }

    pub(crate) fn child_dirs(&self, dir_path: &str) -> &[String] {
        self.child_dirs.get(dir_path).map_or(&[], |v| v)
    }
}
