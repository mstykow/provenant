use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use super::io::write_bytes_atomically;
use super::locking::with_exclusive_cache_lock;
use crate::models::FileInfo;
use crate::utils::hash::calculate_sha256;

const INCREMENTAL_MANIFEST_VERSION: u32 = 1;
const MANIFEST_FILE_NAME: &str = "manifest.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStateFingerprint {
    pub size: u64,
    pub modified_seconds: u64,
    pub modified_nanos: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalManifestEntry {
    pub state: FileStateFingerprint,
    pub content_sha256: String,
    pub file_info: FileInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalManifest {
    pub version: u32,
    pub options_fingerprint: String,
    pub entries: BTreeMap<String, IncrementalManifestEntry>,
}

impl IncrementalManifest {
    pub fn new(
        options_fingerprint: String,
        entries: BTreeMap<String, IncrementalManifestEntry>,
    ) -> Self {
        Self {
            version: INCREMENTAL_MANIFEST_VERSION,
            options_fingerprint,
            entries,
        }
    }

    pub fn entry(&self, relative_path: &str) -> Option<&IncrementalManifestEntry> {
        self.entries.get(relative_path)
    }

    pub fn is_compatible_with(&self, options_fingerprint: &str) -> bool {
        self.version == INCREMENTAL_MANIFEST_VERSION
            && self.options_fingerprint == options_fingerprint
    }
}

pub fn incremental_manifest_path(cache_root: &Path, manifest_key: &str) -> PathBuf {
    cache_root
        .join("incremental")
        .join(manifest_key)
        .join(MANIFEST_FILE_NAME)
}

pub fn metadata_fingerprint(metadata: &fs::Metadata) -> Option<FileStateFingerprint> {
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;

    Some(FileStateFingerprint {
        size: metadata.len(),
        modified_seconds: duration.as_secs(),
        modified_nanos: duration.subsec_nanos(),
    })
}

pub fn manifest_entry_matches_path(
    entry: &IncrementalManifestEntry,
    path: &Path,
    metadata: &fs::Metadata,
) -> io::Result<bool> {
    if !metadata_fingerprint(metadata).is_some_and(|fingerprint| fingerprint == entry.state) {
        return Ok(false);
    }

    let bytes = fs::read(path)?;
    Ok(calculate_sha256(&bytes) == entry.content_sha256)
}

pub fn load_incremental_manifest(
    manifest_path: &Path,
    options_fingerprint: &str,
) -> io::Result<Option<IncrementalManifest>> {
    let bytes = match fs::read(manifest_path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };

    let manifest: IncrementalManifest = match serde_json::from_slice(&bytes) {
        Ok(manifest) => manifest,
        Err(_) => return Ok(None),
    };

    if !manifest.is_compatible_with(options_fingerprint) {
        return Ok(None);
    }

    Ok(Some(manifest))
}

pub fn write_incremental_manifest(
    cache_root: &Path,
    manifest_path: &Path,
    manifest: &IncrementalManifest,
) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    with_exclusive_cache_lock(cache_root, || write_bytes_atomically(manifest_path, &bytes))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::models::{FileInfo, FileType};

    fn sample_manifest(options_fingerprint: &str) -> IncrementalManifest {
        let mut entries = BTreeMap::new();
        entries.insert(
            "src/main.rs".to_string(),
            IncrementalManifestEntry {
                state: FileStateFingerprint {
                    size: 12,
                    modified_seconds: 10,
                    modified_nanos: 20,
                },
                content_sha256: "f2ca1bb6c7e907d06dafe4687e579fce9f2b2c8a179a4e7c1f6c5052d4f7d070"
                    .to_string(),
                file_info: FileInfo::new(
                    "main.rs".to_string(),
                    "main".to_string(),
                    ".rs".to_string(),
                    "/tmp/project/src/main.rs".to_string(),
                    FileType::File,
                    None,
                    None,
                    12,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Vec::new(),
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                ),
            },
        );

        IncrementalManifest::new(options_fingerprint.to_string(), entries)
    }

    #[test]
    fn test_load_incremental_manifest_returns_none_for_incompatible_options() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let manifest_path = incremental_manifest_path(temp_dir.path(), "abc123");
        let manifest = sample_manifest("options-v1");

        write_incremental_manifest(temp_dir.path(), &manifest_path, &manifest)
            .expect("write manifest");

        let loaded =
            load_incremental_manifest(&manifest_path, "options-v2").expect("load manifest");

        assert!(loaded.is_none());
    }

    #[test]
    fn test_write_and_load_incremental_manifest_round_trip() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let manifest_path = incremental_manifest_path(temp_dir.path(), "abc123");
        let manifest = sample_manifest("options-v1");

        write_incremental_manifest(temp_dir.path(), &manifest_path, &manifest)
            .expect("write manifest");

        let loaded = load_incremental_manifest(&manifest_path, "options-v1")
            .expect("load manifest")
            .expect("expected manifest");

        assert_eq!(loaded.entries.len(), 1);
        assert!(loaded.entry("src/main.rs").is_some());
    }

    #[test]
    fn test_manifest_entry_matches_path_detects_content_changes() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = temp_dir.path().join("src/main.rs");
        fs::create_dir_all(file_path.parent().expect("parent")).expect("create parent");
        fs::write(&file_path, "fn main() {}\n").expect("write file");
        let metadata = fs::metadata(&file_path).expect("metadata");

        let entry = IncrementalManifestEntry {
            state: metadata_fingerprint(&metadata).expect("fingerprint"),
            content_sha256: "not-the-real-hash".to_string(),
            file_info: FileInfo::new(
                "main.rs".to_string(),
                "main".to_string(),
                ".rs".to_string(),
                file_path.to_string_lossy().to_string(),
                FileType::File,
                None,
                None,
                metadata.len(),
                None,
                None,
                None,
                None,
                None,
                Vec::new(),
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ),
        };

        assert!(!manifest_entry_matches_path(&entry, &file_path, &metadata).expect("compare path"));
    }
}
