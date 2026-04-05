use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Result, anyhow};
use rusqlite::{Connection, OpenFlags};

use super::BlobReader;

const SQLITE_HEADER_MAGIC: &[u8] = b"SQLite format 3\0";

pub(crate) struct SqliteDatabase {
    connection: Connection,
}

impl SqliteDatabase {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let mut header = [0_u8; 16];
        File::open(path)?.read_exact(&mut header)?;
        if header != SQLITE_HEADER_MAGIC {
            return Err(anyhow!("RPM sqlite database missing SQLite header magic"));
        }

        let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(Self { connection })
    }
}

impl BlobReader for SqliteDatabase {
    fn read_blobs(&mut self) -> Result<Vec<Vec<u8>>> {
        let mut statement = self
            .connection
            .prepare("SELECT blob FROM Packages ORDER BY hnum")?;
        let rows = statement.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
        let mut blobs = Vec::new();
        for row in rows {
            blobs.push(row?);
        }
        Ok(blobs)
    }
}
