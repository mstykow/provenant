// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs::{self, File};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result, anyhow};

use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_FIELD_LENGTH, MAX_ITERATION_COUNT, MAX_MANIFEST_SIZE};

use super::BlobReader;

const SLOT_ENTRIES_PER_PAGE: u32 = 4096 / 16;
const HEADER_MAGIC: u32 =
    ('R' as u32) | ('p' as u32) << 8 | ('m' as u32) << 16 | ('P' as u32) << 24;
const DB_VERSION: u32 = 0;
const BLOB_BLOCK_SIZE: u32 = 16;
const SLOT_MAGIC: u32 = ('S' as u32) | ('l' as u32) << 8 | ('o' as u32) << 16 | ('t' as u32) << 24;
const BLOB_MAGIC_START: u32 =
    ('B' as u32) | ('l' as u32) << 8 | ('b' as u32) << 16 | ('S' as u32) << 24;

struct NdbHeader {
    header_magic: u32,
    db_version: u32,
    _generation: u32,
    slot_page_count: u32,
    _next_package_index: u32,
    _unused: [u32; 3],
}

impl NdbHeader {
    fn read(reader: &mut BufReader<File>) -> Result<Self> {
        Ok(Self {
            header_magic: read_u32_le(reader)?,
            db_version: read_u32_le(reader)?,
            _generation: read_u32_le(reader)?,
            slot_page_count: read_u32_le(reader)?,
            _next_package_index: read_u32_le(reader)?,
            _unused: [
                read_u32_le(reader)?,
                read_u32_le(reader)?,
                read_u32_le(reader)?,
            ],
        })
    }
}

#[derive(Clone)]
struct NdbSlotEntry {
    slot_magic: u32,
    package_index: u32,
    block_offset: u32,
    block_count: u32,
}

impl NdbSlotEntry {
    fn read(reader: &mut BufReader<File>) -> Result<Self> {
        Ok(Self {
            slot_magic: read_u32_le(reader)?,
            package_index: read_u32_le(reader)?,
            block_offset: read_u32_le(reader)?,
            block_count: read_u32_le(reader)?,
        })
    }
}

struct NdbBlobHeader {
    blob_magic: u32,
    package_index: u32,
    _generation_or_checksum: u32,
    blob_length: u32,
}

impl NdbBlobHeader {
    fn read(reader: &mut BufReader<File>) -> Result<Self> {
        Ok(Self {
            blob_magic: read_u32_le(reader)?,
            package_index: read_u32_le(reader)?,
            _generation_or_checksum: read_u32_le(reader)?,
            blob_length: read_u32_le(reader)?,
        })
    }
}

pub(crate) struct NdbDatabase {
    reader: BufReader<File>,
    slots: Vec<NdbSlotEntry>,
}

impl NdbDatabase {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let file_size = fs::metadata(path)
            .with_context(|| format!("RPM NDB metadata check failed for {:?}", path))?
            .len();
        if file_size > MAX_MANIFEST_SIZE {
            return Err(anyhow!(
                "RPM NDB file exceeds safety limit: {} bytes (max {} bytes)",
                file_size,
                MAX_MANIFEST_SIZE
            ));
        }

        let mut reader = BufReader::new(File::open(path)?);
        let header = NdbHeader::read(&mut reader)?;
        if header.header_magic != HEADER_MAGIC {
            return Err(anyhow!("RPM NDB header magic mismatch"));
        }
        if header.db_version != DB_VERSION || header.slot_page_count == 0 {
            return Err(anyhow!(
                "RPM NDB header has unsupported version or empty slot table"
            ));
        }
        if header.slot_page_count > 2048 {
            return Err(anyhow!(
                "RPM NDB slot page count exceeds safety limit: {}",
                header.slot_page_count
            ));
        }

        let slot_count = header.slot_page_count * SLOT_ENTRIES_PER_PAGE - 2;
        let effective_slot_count = if slot_count as usize > MAX_ITERATION_COUNT {
            warn!(
                "RPM NDB slot count {} exceeds iteration cap {}, capping",
                slot_count, MAX_ITERATION_COUNT
            );
            MAX_ITERATION_COUNT as u32
        } else {
            slot_count
        };
        let mut slots = Vec::with_capacity(effective_slot_count as usize);
        for _ in 0..effective_slot_count {
            slots.push(NdbSlotEntry::read(&mut reader)?);
        }

        Ok(Self { reader, slots })
    }
}

impl BlobReader for NdbDatabase {
    fn read_blobs(&mut self) -> Result<Vec<Vec<u8>>> {
        let mut blobs = Vec::new();
        for slot in &self.slots {
            if slot.slot_magic != SLOT_MAGIC {
                return Err(anyhow!("RPM NDB slot magic mismatch: {}", slot.slot_magic));
            }
            if slot.package_index == 0 {
                continue;
            }

            let offset = u64::from(slot.block_offset * BLOB_BLOCK_SIZE);
            self.reader.seek(SeekFrom::Start(offset))?;

            let blob_header = NdbBlobHeader::read(&mut self.reader)?;
            if blob_header.blob_magic != BLOB_MAGIC_START {
                return Err(anyhow!(
                    "RPM NDB blob magic mismatch for package {}",
                    slot.package_index
                ));
            }
            if blob_header.package_index != slot.package_index {
                return Err(anyhow!(
                    "RPM NDB package index mismatch: slot={} blob={}",
                    slot.package_index,
                    blob_header.package_index
                ));
            }

            if blob_header.blob_length as usize > MAX_FIELD_LENGTH {
                warn!(
                    "RPM NDB blob length {} exceeds safety cap for package {}, skipping",
                    blob_header.blob_length, slot.package_index
                );
                continue;
            }

            let mut blob = vec![0_u8; blob_header.blob_length as usize];
            self.reader.read_exact(&mut blob)?;

            let consumed_blocks = ((16 + blob.len() + 12) as u32).div_ceil(BLOB_BLOCK_SIZE);
            if slot.block_count != 0 && slot.block_count < consumed_blocks {
                return Err(anyhow!(
                    "RPM NDB block count too small for package {}: declared={} consumed={}",
                    slot.package_index,
                    slot.block_count,
                    consumed_blocks
                ));
            }

            blobs.push(blob);
        }

        Ok(blobs)
    }
}

fn read_u32_le(reader: &mut BufReader<File>) -> Result<u32> {
    let mut bytes = [0_u8; 4];
    reader
        .read_exact(&mut bytes)
        .context("RPM NDB structured read exceeded input")?;
    Ok(u32::from_le_bytes(bytes))
}
