use crate::models::FileInfo;
use std::fs::{self, File};
use std::io::{Read, Write};
use tempfile::TempDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryMode {
    CollectFirst,
    StreamUnlimited,
    Limit(usize),
}

impl std::fmt::Display for MemoryMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryMode::CollectFirst => write!(f, "0"),
            MemoryMode::StreamUnlimited => write!(f, "-1"),
            MemoryMode::Limit(n) => write!(f, "{n}"),
        }
    }
}

pub(super) fn retain_or_spill_chunk(
    chunk: Vec<FileInfo>,
    retained_files: &mut Vec<FileInfo>,
    spill_store: &mut Option<FileInfoSpillStore>,
    memory_limit: usize,
) {
    if memory_limit == 0 {
        spill_store
            .get_or_insert_with(FileInfoSpillStore::new)
            .spill(chunk);
        return;
    }

    let remaining_capacity = memory_limit.saturating_sub(retained_files.len());
    if remaining_capacity >= chunk.len() && spill_store.is_none() {
        retained_files.extend(chunk);
        return;
    }

    let mut chunk_iter = chunk.into_iter();
    retained_files.extend(chunk_iter.by_ref().take(remaining_capacity));
    let overflow: Vec<FileInfo> = chunk_iter.collect();
    if !overflow.is_empty() {
        spill_store
            .get_or_insert_with(FileInfoSpillStore::new)
            .spill(overflow);
    }
}

pub(super) struct FileInfoSpillStore {
    temp_dir: TempDir,
    batch_index: usize,
}

impl FileInfoSpillStore {
    fn new() -> Self {
        Self {
            temp_dir: TempDir::new().expect("create spill dir"),
            batch_index: 0,
        }
    }

    fn spill(&mut self, files: Vec<FileInfo>) {
        let path = self
            .temp_dir
            .path()
            .join(format!("batch-{:06}.json.zst", self.batch_index));
        self.batch_index += 1;

        let payload = serde_json::to_vec(&files).expect("encode spilled file batch");
        let file = File::create(path).expect("create spill batch file");
        let mut encoder = zstd::Encoder::new(file, 3).expect("create spill encoder");
        encoder
            .write_all(&payload)
            .expect("write spilled file batch");
        encoder.finish().expect("finish spill encoder");
    }

    pub(super) fn load_all(self) -> Vec<FileInfo> {
        let mut paths: Vec<_> = fs::read_dir(self.temp_dir.path())
            .expect("read spill dir")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        paths.sort();

        let mut files = Vec::new();
        for path in paths {
            let file = File::open(path).expect("open spill batch");
            let mut decoder = zstd::Decoder::new(file).expect("create spill decoder");
            let mut payload = Vec::new();
            decoder.read_to_end(&mut payload).expect("read spill batch");
            let mut batch: Vec<FileInfo> =
                serde_json::from_slice(&payload).expect("decode spilled file batch");
            files.append(&mut batch);
        }
        files
    }
}
