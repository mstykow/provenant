use std::collections::HashSet;
use std::hash::Hash;
use std::io::Cursor;
use std::mem;

use anyhow::{Context, Result, anyhow};

use super::tags::{
    HEADER_I18NTABLE, RPM_I18NSTRING_TYPE, RPM_MAX_TYPE, RPM_MIN_TYPE, RPM_STRING_ARRAY_TYPE,
    RPM_STRING_TYPE, RPMTAG_HEADERI18NTABLE, RPMTAG_HEADERIMAGE, RPMTAG_HEADERIMMUTABLE,
    RPMTAG_HEADERSIGNATURES,
};

const REGION_TAG_COUNT: i32 = mem::size_of::<EntryInfo>() as i32;
const REGION_TAG_TYPE: u32 = 7;
const HEADER_MAX_BYTES: usize = 256 * 1024 * 1024;

const TYPE_SIZES: [i32; 16] = [1, 1, 1, 2, 4, 8, -1, 1, -1, -1, 0, 0, 0, 0, 0, 0];
const TYPE_ALIGN: [i32; 16] = [1, 1, 1, 2, 4, 8, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0];

#[derive(Clone, Debug)]
pub(crate) struct EntryInfo {
    pub(crate) tag: i32,
    pub(crate) kind: u32,
    pub(crate) offset: i32,
    pub(crate) count: u32,
}

impl EntryInfo {
    fn swap_be(&self) -> EntryInfo {
        EntryInfo {
            tag: i32::from_be(self.tag),
            kind: u32::from_be(self.kind),
            offset: i32::from_be(self.offset),
            count: u32::from_be(self.count),
        }
    }
}

#[derive(Debug)]
pub(crate) struct IndexEntry {
    pub(crate) info: EntryInfo,
    pub(crate) data: Vec<u8>,
}

impl IndexEntry {
    pub(crate) fn read_i32(&self) -> Result<i32> {
        let bytes: [u8; 4] = self
            .data
            .get(..4)
            .context("expected 4 bytes for int32 entry")?
            .try_into()
            .map_err(|_| anyhow!("failed to read int32 entry bytes"))?;
        Ok(i32::from_be_bytes(bytes))
    }

    pub(crate) fn read_string(&self) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.data)
            .trim_end_matches('\0')
            .to_string())
    }

    pub(crate) fn read_i32_array(&self) -> Result<Vec<i32>> {
        let mut values = Vec::new();
        for chunk in self.data.chunks_exact(4).take(self.info.count as usize) {
            values.push(i32::from_be_bytes(
                chunk
                    .try_into()
                    .map_err(|_| anyhow!("failed to parse int32 array chunk"))?,
            ));
        }
        Ok(values)
    }

    pub(crate) fn read_string_array(&self) -> Result<Vec<String>> {
        Ok(self
            .data
            .split(|&byte| byte == 0)
            .filter(|slice| !slice.is_empty())
            .map(|slice| String::from_utf8_lossy(slice).into_owned())
            .collect())
    }
}

impl Hash for IndexEntry {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.info.tag.hash(state);
    }
}

impl PartialEq for IndexEntry {
    fn eq(&self, other: &Self) -> bool {
        self.info.tag == other.info.tag
    }
}

impl Eq for IndexEntry {}

pub(crate) struct HeaderBlob {
    entry_infos: Vec<EntryInfo>,
    index_length: i32,
    data_length: i32,
    data_start: i32,
    data_end: i32,
    region_tag: i32,
    region_index_length: i32,
    region_data_length: i32,
}

impl HeaderBlob {
    pub(crate) fn parse(data: &[u8]) -> Result<HeaderBlob> {
        let mut cursor = Cursor::new(data);
        let index_length = read_be_i32(&mut cursor)?;
        let data_length = read_be_i32(&mut cursor)?;
        let entry_info_size = mem::size_of::<EntryInfo>() as i32;
        let data_start = 8 + index_length * entry_info_size;
        let total_length = data_start + data_length;
        let data_end = data_start + data_length;
        if index_length < 1 {
            return Err(anyhow!("RPM header blob has no index entries"));
        }
        if total_length >= HEADER_MAX_BYTES as i32 {
            return Err(anyhow!(
                "RPM header blob too large: total={} index_length={} data_length={}",
                total_length,
                index_length,
                data_length
            ));
        }

        let mut entry_infos = Vec::with_capacity(index_length as usize);
        for _ in 0..index_length {
            entry_infos.push(read_entry_info_le(&mut cursor)?);
        }

        let mut blob = HeaderBlob {
            entry_infos,
            index_length,
            data_length,
            data_start,
            data_end,
            region_tag: 0,
            region_index_length: 0,
            region_data_length: 0,
        };
        blob.verify_region(data)?;
        blob.verify_entries(data)?;
        Ok(blob)
    }

    pub(crate) fn import_entries(&self, data: &[u8]) -> Result<Vec<IndexEntry>> {
        let first = self
            .entry_infos
            .first()
            .context("missing RPM header entry")?
            .swap_be();

        let (mut entries, computed_length) = if first.tag >= RPMTAG_HEADERI18NTABLE {
            swab_region(
                data,
                self.entry_infos.clone(),
                0,
                self.data_start,
                self.data_end,
            )?
        } else {
            let region_index_length = if first.offset == 0 {
                self.index_length
            } else {
                self.region_index_length
            };
            let (mut entries, mut computed_length) = swab_region(
                data,
                self.entry_infos[1..region_index_length as usize].to_vec(),
                0,
                self.data_start,
                self.data_end,
            )?;
            if computed_length < 0 {
                return Err(anyhow!("RPM region length became negative"));
            }
            if self.region_index_length < self.entry_infos.len() as i32 - 1 {
                let (extra_entries, dribble_length) = swab_region(
                    data,
                    self.entry_infos[region_index_length as usize..].to_vec(),
                    computed_length,
                    self.data_start,
                    self.data_end,
                )?;
                if dribble_length < 0 {
                    return Err(anyhow!("RPM dribble region length became negative"));
                }
                entries.extend(extra_entries);
                entries = entries
                    .into_iter()
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                computed_length = dribble_length;
            }
            computed_length += mem::size_of::<EntryInfo>() as i32;
            (entries, computed_length)
        };

        entries.sort_by_key(|entry| entry.info.offset);
        if computed_length != self.data_length {
            return Err(anyhow!(
                "RPM header data length mismatch: computed={} expected={}",
                computed_length,
                self.data_length
            ));
        }
        Ok(entries)
    }

    fn verify_region(&mut self, data: &[u8]) -> Result<()> {
        let mut entry = self
            .entry_infos
            .first()
            .context("missing RPM header region entry")?
            .swap_be();
        let region_tag = if [
            RPMTAG_HEADERIMAGE,
            RPMTAG_HEADERSIGNATURES,
            RPMTAG_HEADERIMMUTABLE,
        ]
        .contains(&entry.tag)
        {
            entry.tag
        } else {
            0
        };

        if entry.tag != region_tag {
            return Ok(());
        }
        if !(entry.kind == REGION_TAG_TYPE && entry.count == REGION_TAG_COUNT as u32) {
            return Err(anyhow!("invalid RPM region tag header"));
        }
        if is_out_of_range(self.data_length, entry.offset + REGION_TAG_COUNT) {
            return Err(anyhow!("invalid RPM region tag offset"));
        }

        let region_end = self.data_start + entry.offset;
        let trailer = parse_entry_info_le(
            data.get(region_end as usize..(region_end + REGION_TAG_COUNT) as usize)
                .context("invalid RPM region trailer slice")?,
        )?;

        self.region_data_length = region_end + REGION_TAG_COUNT - self.data_start;
        if region_tag == RPMTAG_HEADERSIGNATURES && entry.tag == RPMTAG_HEADERIMAGE {
            entry.tag = RPMTAG_HEADERSIGNATURES;
        }
        if !(entry.tag == region_tag
            && entry.kind == REGION_TAG_TYPE
            && entry.count == REGION_TAG_COUNT as u32)
        {
            return Err(anyhow!("invalid RPM region trailer header"));
        }

        let mut trailer = trailer.swap_be();
        trailer.offset = -trailer.offset;
        self.region_index_length = trailer.offset / REGION_TAG_COUNT;
        if trailer.offset % REGION_TAG_COUNT != 0
            || is_out_of_range(self.index_length, self.region_index_length)
            || is_out_of_range(self.data_length, self.region_data_length)
        {
            return Err(anyhow!("invalid RPM region size metadata"));
        }
        self.region_tag = region_tag;
        Ok(())
    }

    fn verify_entries(&self, data: &[u8]) -> Result<()> {
        let mut end = 0;
        let entry_offset = usize::from(self.region_tag != 0);
        for entry in &self.entry_infos[entry_offset..] {
            let info = entry.swap_be();
            if end > info.offset {
                return Err(anyhow!("RPM header entry offsets are not sorted"));
            }
            if is_reserved_tag(info.tag) {
                return Err(anyhow!("invalid RPM header tag {}", info.tag));
            }
            if is_invalid_type(info.kind) {
                return Err(anyhow!("invalid RPM header type {}", info.kind));
            }
            if is_misaligned(info.kind, info.offset) {
                return Err(anyhow!(
                    "misaligned RPM header entry offset {}",
                    info.offset
                ));
            }
            if is_out_of_range(self.data_length, info.offset) {
                return Err(anyhow!("RPM header entry offset out of range"));
            }

            let length = compute_data_length(
                data,
                info.kind,
                info.count,
                self.data_start + info.offset,
                self.data_end,
            );
            end = info.offset + length as i32;
            if is_out_of_range(self.data_length, end) || length <= 0 {
                return Err(anyhow!("invalid RPM header entry length"));
            }
        }

        Ok(())
    }
}

fn swab_region(
    data: &[u8],
    entry_infos: Vec<EntryInfo>,
    mut running_length: i32,
    data_start: i32,
    data_end: i32,
) -> Result<(Vec<IndexEntry>, i32)> {
    let mut entries = Vec::new();
    for (index, entry_info) in entry_infos.iter().enumerate() {
        let info = entry_info.swap_be();
        let start = data_start + info.offset;
        if start >= data_end {
            return Err(anyhow!("RPM entry data offset is outside payload"));
        }

        let length =
            if index < entry_infos.len() - 1 && TYPE_SIZES.get(info.kind as usize) == Some(&-1) {
                let next_offset = entry_infos[index + 1].swap_be().offset;
                (next_offset - info.offset) as isize
            } else {
                compute_data_length(data, info.kind, info.count, start, data_end)
            };
        if length < 0 {
            return Err(anyhow!("RPM entry data length is invalid"));
        }

        let end = start as isize + length;
        entries.push(IndexEntry {
            info: info.clone(),
            data: data[start as usize..end as usize].to_vec(),
        });
        running_length += length as i32 + alignment_padding(info.kind, running_length as u32);
    }

    Ok((entries, running_length))
}

fn compute_data_length(data: &[u8], kind: u32, count: u32, start: i32, data_end: i32) -> isize {
    match kind {
        RPM_STRING_TYPE if count != 1 => -1,
        RPM_STRING_TYPE => string_tag_length(data, 1, start, data_end),
        RPM_STRING_ARRAY_TYPE | RPM_I18NSTRING_TYPE => {
            string_tag_length(data, count, start, data_end)
        }
        _ => {
            if TYPE_SIZES.get(kind as usize) == Some(&-1) {
                return -1;
            }
            let size = TYPE_SIZES.get((kind & 0xf) as usize).copied().unwrap_or(0) * count as i32;
            if size < 0 || (data_end > 0 && start + size > data_end) {
                -1
            } else {
                size as isize
            }
        }
    }
}

fn string_tag_length(data: &[u8], count: u32, start: i32, data_end: i32) -> isize {
    if start >= data_end {
        return -1;
    }
    let mut length = 0isize;
    for _ in 0..count {
        let offset = start + length as i32;
        if offset > data.len() as i32 {
            return -1;
        }
        let Some(position) = data[offset as usize..data_end as usize]
            .iter()
            .position(|&byte| byte == 0)
        else {
            return -1;
        };
        length += position as isize + 1;
    }
    length
}

fn alignment_padding(kind: u32, current_length: u32) -> i32 {
    match TYPE_SIZES.get(kind as usize) {
        Some(&size) if size > 1 => {
            let diff = size - (current_length as i32 % size);
            if diff == size { 0 } else { diff }
        }
        _ => 0,
    }
}

fn is_out_of_range(length: i32, offset: i32) -> bool {
    offset < 0 || offset > length
}

fn is_reserved_tag(tag: i32) -> bool {
    tag < HEADER_I18NTABLE
}

fn is_invalid_type(kind: u32) -> bool {
    !(RPM_MIN_TYPE..=RPM_MAX_TYPE).contains(&kind)
}

fn is_misaligned(kind: u32, offset: i32) -> bool {
    let align = TYPE_ALIGN.get(kind as usize).copied().unwrap_or(0);
    offset & (align - 1) != 0
}

fn read_be_i32(cursor: &mut Cursor<&[u8]>) -> Result<i32> {
    let mut bytes = [0_u8; 4];
    std::io::Read::read_exact(cursor, &mut bytes)?;
    Ok(i32::from_be_bytes(bytes))
}

fn read_entry_info_le(cursor: &mut Cursor<&[u8]>) -> Result<EntryInfo> {
    let mut bytes = [0_u8; mem::size_of::<EntryInfo>()];
    std::io::Read::read_exact(cursor, &mut bytes)?;
    parse_entry_info_le(&bytes)
}

fn parse_entry_info_le(data: &[u8]) -> Result<EntryInfo> {
    let mut offset = 0;
    Ok(EntryInfo {
        tag: read_i32_le(data, &mut offset)?,
        kind: read_u32_le(data, &mut offset)?,
        offset: read_i32_le(data, &mut offset)?,
        count: read_u32_le(data, &mut offset)?,
    })
}

fn read_i32_le(data: &[u8], offset: &mut usize) -> Result<i32> {
    let bytes: [u8; 4] = data
        .get(*offset..*offset + 4)
        .context("RPM entry read exceeded input")?
        .try_into()
        .map_err(|_| anyhow!("RPM entry read had wrong width"))?;
    *offset += 4;
    Ok(i32::from_le_bytes(bytes))
}

fn read_u32_le(data: &[u8], offset: &mut usize) -> Result<u32> {
    let bytes: [u8; 4] = data
        .get(*offset..*offset + 4)
        .context("RPM entry read exceeded input")?
        .try_into()
        .map_err(|_| anyhow!("RPM entry read had wrong width"))?;
    *offset += 4;
    Ok(u32::from_le_bytes(bytes))
}
