mod bdb;
mod entry;
mod ndb;
mod package;
#[cfg(feature = "rpm-sqlite")]
mod sqlite;
mod tags;

use std::path::Path;

use anyhow::Result;

use self::bdb::BdbDatabase;
use self::entry::HeaderBlob;
use self::ndb::NdbDatabase;
pub(crate) use self::package::InstalledRpmPackage;
use self::package::parse_installed_rpm_package;
#[cfg(feature = "rpm-sqlite")]
use self::sqlite::SqliteDatabase;

pub(crate) enum InstalledRpmDbKind {
    Bdb,
    Ndb,
    Sqlite,
}

trait BlobReader {
    fn read_blobs(&mut self) -> Result<Vec<Vec<u8>>>;
}

pub(crate) fn read_installed_rpm_packages(
    path: &Path,
    kind: InstalledRpmDbKind,
) -> Result<Vec<InstalledRpmPackage>> {
    let mut reader: Box<dyn BlobReader> = match kind {
        InstalledRpmDbKind::Bdb => Box::new(BdbDatabase::open(path)?),
        InstalledRpmDbKind::Ndb => Box::new(NdbDatabase::open(path)?),
        InstalledRpmDbKind::Sqlite => {
            #[cfg(feature = "rpm-sqlite")]
            {
                Box::new(SqliteDatabase::open(path)?)
            }
            #[cfg(not(feature = "rpm-sqlite"))]
            {
                return Err(anyhow::anyhow!(
                    "RPM SQLite support is disabled at compile time; enable the `rpm-sqlite` feature"
                ));
            }
        }
    };

    reader
        .read_blobs()?
        .into_iter()
        .map(|blob| {
            let header = HeaderBlob::parse(&blob)?;
            let entries = header.import_entries(&blob)?;
            parse_installed_rpm_package(entries)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "rpm-sqlite")]
    use std::fs;
    use std::fs::File;
    #[cfg(feature = "rpm-sqlite")]
    use std::io::Write;
    #[cfg(feature = "rpm-sqlite")]
    use std::path::Path;

    use liblzma::read::XzDecoder;
    use tar::Archive;
    use tempfile::tempdir;

    #[cfg(feature = "rpm-sqlite")]
    use rusqlite::Connection;

    use super::{InstalledRpmDbKind, read_installed_rpm_packages};

    #[test]
    fn test_read_installed_rpm_packages_from_bdb_fixture() {
        let temp_dir = tempdir().expect("temporary extraction dir should exist");
        let archive_file = File::open("testdata/rpm/bdb-fedora-rootfs.tar.xz")
            .expect("Fedora BDB archive should exist");
        let archive = XzDecoder::new(archive_file);
        Archive::new(archive)
            .unpack(temp_dir.path())
            .expect("Fedora BDB archive should extract");

        let packages = read_installed_rpm_packages(
            &temp_dir.path().join("rootfs/var/lib/rpm/Packages"),
            InstalledRpmDbKind::Bdb,
        )
        .expect("BDB fixture should parse");

        assert!(!packages.is_empty());
        assert!(packages.iter().any(|package| package.name == "libgcc"));
    }

    #[cfg(feature = "rpm-sqlite")]
    #[test]
    fn test_read_installed_rpm_packages_from_synthetic_ndb_fixture() {
        let temp_dir = tempdir().expect("temporary fixture dir should exist");
        let db_path = temp_dir.path().join("Packages.db");
        write_synthetic_ndb_fixture(&db_path).expect("synthetic NDB fixture should be written");

        let packages = read_installed_rpm_packages(&db_path, InstalledRpmDbKind::Ndb)
            .expect("NDB fixture should parse");

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "libgcc");
    }

    #[cfg(feature = "rpm-sqlite")]
    #[test]
    fn test_read_installed_rpm_packages_from_sqlite_fixture() {
        let packages = read_installed_rpm_packages(
            Path::new("testdata/rpm/rpmdb.sqlite"),
            InstalledRpmDbKind::Sqlite,
        )
        .expect("SQLite fixture should parse");

        assert!(!packages.is_empty());
        assert!(packages.iter().any(|package| package.name == "libgcc"));
    }

    #[cfg(feature = "rpm-sqlite")]
    fn write_synthetic_ndb_fixture(path: &Path) -> anyhow::Result<()> {
        const HEADER_MAGIC: u32 =
            ('R' as u32) | ('p' as u32) << 8 | ('m' as u32) << 16 | ('P' as u32) << 24;
        const SLOT_MAGIC: u32 =
            ('S' as u32) | ('l' as u32) << 8 | ('o' as u32) << 16 | ('t' as u32) << 24;
        const BLOB_MAGIC_START: u32 =
            ('B' as u32) | ('l' as u32) << 8 | ('b' as u32) << 16 | ('S' as u32) << 24;
        const BLOB_MAGIC_END: u32 =
            ('B' as u32) | ('l' as u32) << 8 | ('b' as u32) << 16 | ('E' as u32) << 24;
        const SLOT_ENTRIES: usize = 254;
        const BLOCK_SIZE: u32 = 16;

        let connection = Connection::open("testdata/rpm/rpmdb.sqlite")?;
        let blob: Vec<u8> = connection.query_row(
            "SELECT blob FROM Packages ORDER BY hnum LIMIT 1",
            [],
            |row| row.get(0),
        )?;

        let block_count = ((16 + blob.len() as u32 + 12).div_ceil(BLOCK_SIZE)).max(1);
        let mut data = Vec::new();
        push_u32_le(&mut data, HEADER_MAGIC);
        push_u32_le(&mut data, 0);
        push_u32_le(&mut data, 0);
        push_u32_le(&mut data, 1);
        push_u32_le(&mut data, 2);
        push_u32_le(&mut data, 0);
        push_u32_le(&mut data, 0);
        push_u32_le(&mut data, 0);

        push_u32_le(&mut data, SLOT_MAGIC);
        push_u32_le(&mut data, 1);
        push_u32_le(&mut data, 256);
        push_u32_le(&mut data, block_count);

        for _ in 1..SLOT_ENTRIES {
            push_u32_le(&mut data, SLOT_MAGIC);
            push_u32_le(&mut data, 0);
            push_u32_le(&mut data, 0);
            push_u32_le(&mut data, 0);
        }

        push_u32_le(&mut data, BLOB_MAGIC_START);
        push_u32_le(&mut data, 1);
        push_u32_le(&mut data, 0);
        push_u32_le(&mut data, blob.len() as u32);
        data.extend_from_slice(&blob);
        push_u32_le(&mut data, 0);
        push_u32_le(&mut data, blob.len() as u32);
        push_u32_le(&mut data, BLOB_MAGIC_END);

        while data.len() % BLOCK_SIZE as usize != 0 {
            data.push(0);
        }

        fs::write(path, data)?;
        Ok(())
    }

    #[cfg(feature = "rpm-sqlite")]
    fn push_u32_le(buffer: &mut Vec<u8>, value: u32) {
        buffer
            .write_all(&value.to_le_bytes())
            .expect("write to vec");
    }
}
