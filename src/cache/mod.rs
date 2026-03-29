use std::path::Path;

use glob::Pattern;

mod config;
mod io;
mod license_index_cache;
mod metadata;
mod paths;
mod scan_cache;

pub use config::{CACHE_DIR_ENV_VAR, CacheConfig, CacheKind, CacheKinds, DEFAULT_CACHE_DIR_NAME};
pub use license_index_cache::{LicenseIndexCacheSource, load_or_build_embedded_license_index};
pub use scan_cache::{CachedScanFindings, read_cached_findings, write_cached_findings};

pub fn build_collection_exclude_patterns(scan_root: &Path, cache_root: &Path) -> Vec<Pattern> {
    let mut patterns = Vec::new();

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
