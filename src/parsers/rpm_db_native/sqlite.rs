use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Result, anyhow};
use rusqlite::{Connection, OpenFlags};

use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, MAX_MANIFEST_SIZE};

use super::BlobReader;

const SQLITE_HEADER_MAGIC: &[u8] = b"SQLite format 3\0";

pub(crate) struct SqliteDatabase {
    connection: Connection,
}

impl SqliteDatabase {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let metadata = fs::metadata(path)?;
        if metadata.len() > MAX_MANIFEST_SIZE {
            return Err(anyhow!(
                "RPM sqlite database too large: {} bytes (max {} bytes)",
                metadata.len(),
                MAX_MANIFEST_SIZE
            ));
        }

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
        let mut statement = self.connection.prepare(&format!(
            "SELECT blob FROM Packages ORDER BY hnum LIMIT {}",
            MAX_ITERATION_COUNT
        ))?;
        let rows = statement.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
        let mut blobs = Vec::new();
        for (i, row) in rows.enumerate() {
            if i >= MAX_ITERATION_COUNT {
                warn!("RPM sqlite database row iteration exceeded MAX_ITERATION_COUNT, truncating");
                break;
            }
            blobs.push(row?);
        }
        Ok(blobs)
    }
}
