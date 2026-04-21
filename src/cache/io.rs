// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;

pub fn write_bytes_atomically(path: &Path, payload: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Atomic write path has no parent: {path:?}"),
        )
    })?;

    fs::create_dir_all(parent)?;

    let temp_path = temp_atomic_path(path);
    let result =
        write_bytes_to_temp(&temp_path, payload).and_then(|_| replace_file(&temp_path, path));

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }

    result
}

fn write_bytes_to_temp(temp_path: &Path, payload: &[u8]) -> io::Result<()> {
    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)?;
    temp_file.write_all(payload)?;
    temp_file.sync_all()?;
    Ok(())
}

fn replace_file(temp_path: &Path, final_path: &Path) -> io::Result<()> {
    match fs::rename(temp_path, final_path) {
        Ok(()) => Ok(()),
        Err(err) if final_path.exists() => {
            fs::remove_file(final_path)?;
            fs::rename(temp_path, final_path).map_err(|rename_err| {
                io::Error::new(
                    rename_err.kind(),
                    format!("{err}; replace retry failed: {rename_err}"),
                )
            })
        }
        Err(err) => Err(err),
    }
}

fn temp_atomic_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("snapshot");
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(".tmp-{file_name}-{}", Uuid::new_v4()))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_write_bytes_atomically_round_trip() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path().join("incremental").join("manifest.json");

        write_bytes_atomically(&path, b"hello world").expect("write bytes atomically");

        assert_eq!(fs::read(&path).expect("read bytes"), b"hello world");
    }
}
