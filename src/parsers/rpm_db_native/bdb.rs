use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Result, anyhow};

use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_FIELD_LENGTH, MAX_ITERATION_COUNT, MAX_MANIFEST_SIZE};

use super::BlobReader;

const VALID_PAGE_SIZES: [u32; 8] = [512, 1024, 2048, 4096, 8192, 16384, 32768, 65536];
const HASH_INDEX_ENTRY_SIZE: usize = 2;
const PAGE_HEADER_SIZE: usize = 26;
const HASH_OFF_PAGE_ENTRY_SIZE: usize = 12;

#[repr(u8)]
enum PageType {
    HashUnsorted = 2,
    OffPageIndex = 3,
    Overflow = 7,
    HashMetadata = 8,
    HashSorted = 13,
}

pub(crate) struct BdbDatabase {
    file: File,
    metadata: HashMetadataPage,
}

impl BdbDatabase {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let file_metadata = fs::metadata(path)
            .map_err(|e| anyhow!("RPM BDB cannot stat file {:?}: {}", path, e))?;
        if file_metadata.len() > MAX_MANIFEST_SIZE {
            return Err(anyhow!(
                "RPM BDB file {:?} is {} bytes, exceeding the {} byte limit",
                path,
                file_metadata.len(),
                MAX_MANIFEST_SIZE
            ));
        }

        let mut file = File::open(path)?;
        let mut page = [0_u8; 512];
        file.read_exact(&mut page)?;
        file.seek(SeekFrom::Start(0))?;

        let metadata = HashMetadataPage::parse(&page)?;
        if !VALID_PAGE_SIZES.contains(&metadata.generic.page_size) {
            return Err(anyhow!(
                "RPM BDB page size is invalid: {}",
                metadata.generic.page_size
            ));
        }
        if metadata.generic.page_type != PageType::HashMetadata as u8 {
            return Err(anyhow!(
                "RPM BDB metadata page type is invalid: {}",
                metadata.generic.page_type
            ));
        }

        Ok(Self { file, metadata })
    }

    fn read_overflow_value(&mut self, entry: HashOffPageEntry) -> Result<Vec<u8>> {
        let page_size = self.metadata.generic.page_size as usize;
        let mut value = Vec::new();
        let mut current_page = entry.page_number as usize;
        let mut depth = 0usize;

        while current_page != 0 {
            depth += 1;
            if depth > MAX_ITERATION_COUNT {
                return Err(anyhow!(
                    "RPM BDB overflow page chain exceeded {} pages",
                    MAX_ITERATION_COUNT
                ));
            }

            self.file
                .seek(SeekFrom::Start((page_size * current_page) as u64))?;
            let mut page = vec![0_u8; page_size];
            self.file.read_exact(&mut page)?;
            let header = HashPageHeader::parse(&page)?;
            if header.page_type != PageType::Overflow as u8 {
                return Err(anyhow!(
                    "RPM BDB overflow page had unexpected type {}",
                    header.page_type
                ));
            }

            let page_bytes = if header.next_page_number == 0 {
                &page[PAGE_HEADER_SIZE..PAGE_HEADER_SIZE + header.free_area_offset as usize]
            } else {
                &page[PAGE_HEADER_SIZE..]
            };
            value.extend_from_slice(page_bytes);

            if value.len() > MAX_FIELD_LENGTH {
                return Err(anyhow!(
                    "RPM BDB overflow value exceeded {} bytes",
                    MAX_FIELD_LENGTH
                ));
            }

            current_page = header.next_page_number as usize;
        }

        if entry.length as usize > value.len() {
            return Err(anyhow!(
                "RPM BDB overflow length exceeds collected bytes: {} > {}",
                entry.length,
                value.len()
            ));
        }
        value.truncate(entry.length as usize);
        Ok(value)
    }
}

impl BlobReader for BdbDatabase {
    fn read_blobs(&mut self) -> Result<Vec<Vec<u8>>> {
        let page_size = self.metadata.generic.page_size as usize;
        let mut values = Vec::new();

        let last_page = self.metadata.generic.last_page_number;
        let effective_last_page = if last_page as usize > MAX_ITERATION_COUNT {
            warn!(
                "RPM BDB last_page_number {} exceeds {}, capping iteration",
                last_page, MAX_ITERATION_COUNT
            );
            MAX_ITERATION_COUNT as u32
        } else {
            last_page
        };

        for _ in 0..=effective_last_page {
            let mut page = vec![0_u8; page_size];
            self.file.read_exact(&mut page)?;
            let page_end_offset = self.file.stream_position()?;

            let header = HashPageHeader::parse(&page)?;
            if header.page_type != PageType::HashUnsorted as u8
                && header.page_type != PageType::HashSorted as u8
            {
                continue;
            }

            for index in hash_page_value_indexes(&page, header.entry_count)? {
                if page.get(index as usize) != Some(&(PageType::OffPageIndex as u8)) {
                    continue;
                }

                let entry = HashOffPageEntry::parse(
                    page.get(index as usize..index as usize + HASH_OFF_PAGE_ENTRY_SIZE)
                        .ok_or_else(|| anyhow!("RPM BDB off-page entry slice is out of bounds"))?,
                )?;
                values.push(self.read_overflow_value(entry)?);
            }

            self.file.seek(SeekFrom::Start(page_end_offset))?;
        }

        Ok(values)
    }
}

fn hash_page_value_indexes(page: &[u8], entry_count: u16) -> Result<Vec<u16>> {
    if !entry_count.is_multiple_of(2) {
        return Err(anyhow!(
            "RPM BDB hash page had odd entry count {}",
            entry_count
        ));
    }

    let capped_entry_count = if entry_count as usize > MAX_ITERATION_COUNT {
        warn!(
            "RPM BDB hash page entry count {} exceeds {}, capping",
            entry_count, MAX_ITERATION_COUNT
        );
        MAX_ITERATION_COUNT as u16
    } else {
        entry_count
    };

    let index_bytes = page
        .get(
            PAGE_HEADER_SIZE
                ..PAGE_HEADER_SIZE + capped_entry_count as usize * HASH_INDEX_ENTRY_SIZE,
        )
        .ok_or_else(|| anyhow!("RPM BDB hash index slice is out of bounds"))?;

    let mut result = Vec::new();
    for chunk in index_bytes.chunks_exact(2 * HASH_INDEX_ENTRY_SIZE) {
        if result.len() >= MAX_ITERATION_COUNT {
            break;
        }
        result.push(u16::from_le_bytes([
            chunk[HASH_INDEX_ENTRY_SIZE],
            chunk[HASH_INDEX_ENTRY_SIZE + 1],
        ]));
    }
    Ok(result)
}

#[derive(Debug)]
struct GenericMetadataPage {
    _lsn: [u8; 8],
    _page_number: u32,
    _magic: u32,
    _version: u32,
    page_size: u32,
    _encryption_alg: u8,
    page_type: u8,
    _meta_flags: u8,
    _unused1: u8,
    _free: u32,
    last_page_number: u32,
    _n_parts: u32,
    _key_count: u32,
    _record_count: u32,
    _flags: u32,
    _unique_file_id: [u8; 19],
}

impl GenericMetadataPage {
    fn parse(data: &[u8]) -> Result<Self> {
        let mut offset = 0;
        Ok(Self {
            _lsn: read_array::<8>(data, &mut offset)?,
            _page_number: read_u32_le(data, &mut offset)?,
            _magic: read_u32_le(data, &mut offset)?,
            _version: read_u32_le(data, &mut offset)?,
            page_size: read_u32_le(data, &mut offset)?,
            _encryption_alg: read_u8(data, &mut offset)?,
            page_type: read_u8(data, &mut offset)?,
            _meta_flags: read_u8(data, &mut offset)?,
            _unused1: read_u8(data, &mut offset)?,
            _free: read_u32_le(data, &mut offset)?,
            last_page_number: read_u32_le(data, &mut offset)?,
            _n_parts: read_u32_le(data, &mut offset)?,
            _key_count: read_u32_le(data, &mut offset)?,
            _record_count: read_u32_le(data, &mut offset)?,
            _flags: read_u32_le(data, &mut offset)?,
            _unique_file_id: read_array::<19>(data, &mut offset)?,
        })
    }
}

#[derive(Debug)]
struct HashMetadataPage {
    generic: GenericMetadataPage,
    _max_bucket: u32,
    _high_mask: u32,
    _low_mask: u32,
    _fill_factor: u32,
    _num_keys: u32,
    _char_key_hash: u32,
}

impl HashMetadataPage {
    fn parse(data: &[u8]) -> Result<Self> {
        let mut offset = 0;
        let generic = GenericMetadataPage::parse(data)?;
        offset += 72;
        Ok(Self {
            generic,
            _max_bucket: read_u32_le(data, &mut offset)?,
            _high_mask: read_u32_le(data, &mut offset)?,
            _low_mask: read_u32_le(data, &mut offset)?,
            _fill_factor: read_u32_le(data, &mut offset)?,
            _num_keys: read_u32_le(data, &mut offset)?,
            _char_key_hash: read_u32_le(data, &mut offset)?,
        })
    }
}

#[derive(Debug)]
struct HashPageHeader {
    _lsn: [u8; 8],
    _page_number: u32,
    _previous_page_number: u32,
    next_page_number: u32,
    entry_count: u16,
    free_area_offset: u16,
    _tree_level: u8,
    page_type: u8,
}

impl HashPageHeader {
    fn parse(data: &[u8]) -> Result<Self> {
        let mut offset = 0;
        Ok(Self {
            _lsn: read_array::<8>(data, &mut offset)?,
            _page_number: read_u32_le(data, &mut offset)?,
            _previous_page_number: read_u32_le(data, &mut offset)?,
            next_page_number: read_u32_le(data, &mut offset)?,
            entry_count: read_u16_le(data, &mut offset)?,
            free_area_offset: read_u16_le(data, &mut offset)?,
            _tree_level: read_u8(data, &mut offset)?,
            page_type: read_u8(data, &mut offset)?,
        })
    }
}

#[derive(Debug)]
struct HashOffPageEntry {
    _page_type: u8,
    _unused: [u8; 3],
    page_number: u32,
    length: u32,
}

impl HashOffPageEntry {
    fn parse(data: &[u8]) -> Result<Self> {
        let mut offset = 0;
        Ok(Self {
            _page_type: read_u8(data, &mut offset)?,
            _unused: read_array::<3>(data, &mut offset)?,
            page_number: read_u32_le(data, &mut offset)?,
            length: read_u32_le(data, &mut offset)?,
        })
    }
}

fn read_u8(data: &[u8], offset: &mut usize) -> Result<u8> {
    let value = *data
        .get(*offset)
        .ok_or_else(|| anyhow!("RPM BDB header read exceeded input"))?;
    *offset += 1;
    Ok(value)
}

fn read_u16_le(data: &[u8], offset: &mut usize) -> Result<u16> {
    let bytes = read_array::<2>(data, offset)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32_le(data: &[u8], offset: &mut usize) -> Result<u32> {
    let bytes = read_array::<4>(data, offset)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_array<const N: usize>(data: &[u8], offset: &mut usize) -> Result<[u8; N]> {
    let bytes: [u8; N] = data
        .get(*offset..*offset + N)
        .ok_or_else(|| anyhow!("RPM BDB structured read exceeded input"))?
        .try_into()
        .map_err(|_| anyhow!("RPM BDB structured read had wrong width"))?;
    *offset += N;
    Ok(bytes)
}
