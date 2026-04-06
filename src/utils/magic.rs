//! File magic byte detection utilities.
//!
//! Provides low-level file format detection by reading and checking magic bytes
//! at the beginning of files. Used by parsers to disambiguate file types that
//! share the same extension (e.g., Alpine .apk vs Android .apk).

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Check if file starts with ZIP magic bytes (PK\x03\x04).
///
/// ZIP format is used by many file types including Android APK, JAR, InstallShield installers, etc.
///
/// # Returns
/// `true` if the file starts with the ZIP signature, `false` otherwise or on IO error.
pub fn is_zip(path: &Path) -> bool {
    check_magic_bytes(path, &[0x50, 0x4B, 0x03, 0x04])
}

pub fn is_gzip(path: &Path) -> bool {
    check_magic_bytes(path, &[0x1F, 0x8B])
}

/// Check if file starts with Squashfs magic bytes.
///
/// Squashfs filesystems can be either little-endian (hsqs) or big-endian (sqsh).
/// This function checks for both variants.
///
/// # Returns
/// `true` if the file starts with either Squashfs signature, `false` otherwise or on IO error.
pub fn is_squashfs(path: &Path) -> bool {
    // Little-endian: hsqs (0x68, 0x73, 0x71, 0x73)
    // Big-endian: sqsh (0x73, 0x71, 0x73, 0x68)
    check_magic_bytes(path, &[0x68, 0x73, 0x71, 0x73])
        || check_magic_bytes(path, &[0x73, 0x71, 0x73, 0x68])
}

/// Check if file contains NSIS installer signature.
///
/// NSIS installers are Windows executables that contain a specific signature string.
/// This function searches the first 8KB of the file for "Nullsoft.NSIS.exehead".
///
/// # Returns
/// `true` if the NSIS signature is found within the first 8KB, `false` otherwise or on IO error.
pub fn is_nsis_installer(path: &Path) -> bool {
    const SEARCH_SIZE: usize = 8192; // 8KB
    const NSIS_SIGNATURE: &[u8] = b"Nullsoft.NSIS.exehead";

    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };

    let mut buffer = vec![0u8; SEARCH_SIZE];
    let bytes_read = match file.read(&mut buffer) {
        Ok(n) => n,
        Err(_) => return false,
    };

    buffer.truncate(bytes_read);

    // Search for NSIS signature in the buffer
    buffer
        .windows(NSIS_SIGNATURE.len())
        .any(|window| window == NSIS_SIGNATURE)
}

/// Helper function to check if a file starts with specific magic bytes.
///
/// Reads only the minimum number of bytes needed for comparison.
fn check_magic_bytes(path: &Path, magic: &[u8]) -> bool {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };

    let mut buffer = vec![0u8; magic.len()];
    match file.read_exact(&mut buffer) {
        Ok(()) => buffer == magic,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_is_zip() {
        // Create a file with ZIP magic bytes
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&[0x50, 0x4B, 0x03, 0x04, 0x00, 0x00])
            .unwrap();
        assert!(is_zip(file.path()));

        // Create a file without ZIP magic bytes
        let mut file2 = NamedTempFile::new().unwrap();
        file2.write_all(&[0x1F, 0x8B, 0x08, 0x00]).unwrap();
        assert!(!is_zip(file2.path()));

        // Non-existent file
        assert!(!is_zip(Path::new("/nonexistent/file.zip")));
    }

    #[test]
    fn test_is_gzip() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&[0x1F, 0x8B, 0x08, 0x00]).unwrap();
        assert!(is_gzip(file.path()));

        let mut file2 = NamedTempFile::new().unwrap();
        file2.write_all(&[0x50, 0x4B, 0x03, 0x04]).unwrap();
        assert!(!is_gzip(file2.path()));
    }

    #[test]
    fn test_is_squashfs_little_endian() {
        // Create a file with Squashfs little-endian magic (hsqs)
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&[0x68, 0x73, 0x71, 0x73, 0x00, 0x00])
            .unwrap();
        assert!(is_squashfs(file.path()));
    }

    #[test]
    fn test_is_squashfs_big_endian() {
        // Create a file with Squashfs big-endian magic (sqsh)
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&[0x73, 0x71, 0x73, 0x68, 0x00, 0x00])
            .unwrap();
        assert!(is_squashfs(file.path()));
    }

    #[test]
    fn test_is_squashfs_negative() {
        // Create a file without Squashfs magic
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&[0x50, 0x4B, 0x03, 0x04]).unwrap();
        assert!(!is_squashfs(file.path()));

        // Non-existent file
        assert!(!is_squashfs(Path::new("/nonexistent/file.squashfs")));
    }

    #[test]
    fn test_is_nsis_installer() {
        // Create a file with NSIS signature at the beginning
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"MZ\x90\x00").unwrap(); // DOS header
        file.write_all(b"Nullsoft.NSIS.exehead").unwrap();
        file.write_all(&[0u8; 100]).unwrap();
        assert!(is_nsis_installer(file.path()));

        // Create a file with NSIS signature in the middle
        let mut file2 = NamedTempFile::new().unwrap();
        file2.write_all(&vec![0u8; 1000]).unwrap();
        file2.write_all(b"Nullsoft.NSIS.exehead").unwrap();
        assert!(is_nsis_installer(file2.path()));

        // Create a file without NSIS signature
        let mut file3 = NamedTempFile::new().unwrap();
        file3.write_all(b"This is not an NSIS installer").unwrap();
        assert!(!is_nsis_installer(file3.path()));

        // Non-existent file
        assert!(!is_nsis_installer(Path::new("/nonexistent/setup.exe")));
    }

    #[test]
    fn test_is_nsis_installer_beyond_8kb() {
        // Create a file with NSIS signature beyond 8KB - should NOT match
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&vec![0u8; 8500]).unwrap();
        file.write_all(b"Nullsoft.NSIS.exehead").unwrap();
        assert!(!is_nsis_installer(file.path()));
    }

    #[test]
    fn test_check_magic_bytes_short_file() {
        // File shorter than expected magic bytes
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&[0x50, 0x4B]).unwrap(); // Only 2 bytes
        assert!(!check_magic_bytes(file.path(), &[0x50, 0x4B, 0x03, 0x04]));
    }

    #[test]
    fn test_check_magic_bytes_empty_file() {
        // Empty file
        let file = NamedTempFile::new().unwrap();
        assert!(!check_magic_bytes(file.path(), &[0x50, 0x4B]));
    }
}
