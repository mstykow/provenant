pub(crate) fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

pub(crate) fn parent_dir_for_lookup(path: &str) -> Option<&str> {
    if path.is_empty() {
        return None;
    }

    path.rsplit_once('/').map(|(parent, _)| parent).or(Some(""))
}

#[cfg(test)]
mod tests {
    use super::{parent_dir, parent_dir_for_lookup};

    #[test]
    fn parent_dir_handles_top_level_paths() {
        assert_eq!(parent_dir("package.json"), "");
        assert_eq!(parent_dir("packages/app/package.json"), "packages/app");
    }

    #[test]
    fn parent_dir_for_lookup_walks_up_to_empty_root() {
        assert_eq!(parent_dir_for_lookup("packages/app"), Some("packages"));
        assert_eq!(parent_dir_for_lookup("packages"), Some(""));
        assert_eq!(parent_dir_for_lookup(""), None);
    }
}
