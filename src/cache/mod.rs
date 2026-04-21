// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use glob::Pattern;

mod config;
mod incremental;
mod io;
pub(crate) mod locking;

pub use config::{CACHE_DIR_ENV_VAR, CacheConfig, DEFAULT_CACHE_DIR_NAME};
pub use incremental::{
    IncrementalManifest, IncrementalManifestEntry, incremental_manifest_path,
    load_incremental_manifest, manifest_entry_matches_path, metadata_fingerprint,
    write_incremental_manifest,
};
pub(crate) use io::write_bytes_atomically;

pub fn build_collection_exclude_patterns(scan_root: &Path, cache_root: &Path) -> Vec<Pattern> {
    let mut patterns = Vec::new();

    for vcs_dir in [".git", ".hg", ".svn"] {
        for pattern in [vcs_dir.to_string(), format!("{vcs_dir}/**")] {
            if let Ok(pattern) = Pattern::new(&pattern) {
                patterns.push(pattern);
            }
        }
    }

    for pattern in [".gitignore", "**/.gitignore"] {
        if let Ok(pattern) = Pattern::new(pattern) {
            patterns.push(pattern);
        }
    }

    if let Ok(relative_cache_root) = cache_root.strip_prefix(scan_root)
        && !relative_cache_root.as_os_str().is_empty()
    {
        for path in [cache_root.to_path_buf(), relative_cache_root.to_path_buf()] {
            let normalized = path.to_string_lossy().replace('\\', "/");
            let escaped = Pattern::escape(&normalized);
            for pattern in [escaped.clone(), format!("{escaped}/**")] {
                if let Ok(pattern) = Pattern::new(&pattern) {
                    patterns.push(pattern);
                }
            }
        }
    }

    patterns
}
