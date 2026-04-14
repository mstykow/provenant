use std::collections::HashSet;
use std::hash::Hash;
use std::io::Cursor;

use anyhow::{Context, Result, anyhow};

use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, MAX_MANIFEST_SIZE, truncate_field};

use super::tags::{
    HEADER_I18NTABLE, RPMTAG_HEADERI18NTABLE, RPMTAG_HEADERIMAGE, RPMTAG_HEADERIMMUTABLE,
    RPMTAG_HEADERSIGNATURES, TagType,
};

const ENTRY_INFO_DISK_SIZE: u32 = 16;
const REGION_TAG_COUNT: u32 = ENTRY_INFO_DISK_SIZE;
const HEADER_MAX_BYTES: usize = MAX_MANIFEST_SIZE as usize;

#[derive(Clone, Debug)]
struct RawEntryInfo {
    tag: u32,
    kind: u32,
    offset: i32,
    count: u32,
}

impl RawEntryInfo {
    fn to_native(&self) -> Result<EntryInfo> {
        Ok(EntryInfo {
            tag: u32::from_be(self.tag),
            kind: TagType::from_raw(u32::from_be(self.kind))?,
            offset: i32::from_be(self.offset),
            count: u32::from_be(self.count),
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct EntryInfo {
    pub(crate) tag: u32,
    pub(crate) kind: TagType,
    pub(crate) offset: i32,
    pub(crate) count: u32,
}

#[derive(Debug)]
pub(crate) struct IndexEntry {
    pub(crate) info: EntryInfo,
    pub(crate) data: Vec<u8>,
}

impl IndexEntry {
    pub(crate) fn read_u32(&self) -> Result<u32> {
        let bytes: [u8; 4] = self
            .data
            .get(..4)
            .context("expected 4 bytes for uint32 entry")?
            .try_into()
            .map_err(|_| anyhow!("failed to read uint32 entry bytes"))?;
        Ok(u32::from_be_bytes(bytes))
    }

    pub(crate) fn read_string(&self) -> Result<String> {
        Ok(truncate_field(
            String::from_utf8_lossy(&self.data)
                .trim_end_matches('\0')
                .to_string(),
        ))
    }

    pub(crate) fn read_u32_array(&self) -> Result<Vec<u32>> {
        if self.info.count as usize > MAX_ITERATION_COUNT {
            warn!(
                "RPM u32 array count {} exceeds MAX_ITERATION_COUNT {}; truncating",
                self.info.count, MAX_ITERATION_COUNT
            );
        }
        let cap = (self.info.count as usize).min(MAX_ITERATION_COUNT);
        let mut values = Vec::with_capacity(cap);
        for chunk in self.data.chunks_exact(4).take(cap) {
            values.push(u32::from_be_bytes(
                chunk
                    .try_into()
                    .map_err(|_| anyhow!("failed to parse uint32 array chunk"))?,
            ));
        }
        Ok(values)
    }

    pub(crate) fn read_string_array(&self) -> Result<Vec<String>> {
        let count = self
            .data
            .split(|&byte| byte == 0)
            .filter(|slice| !slice.is_empty())
            .count();
        if count > MAX_ITERATION_COUNT {
            warn!(
                "RPM string array count {} exceeds MAX_ITERATION_COUNT {}; truncating",
                count, MAX_ITERATION_COUNT
            );
        }
        Ok(self
            .data
            .split(|&byte| byte == 0)
            .filter(|slice| !slice.is_empty())
            .take(MAX_ITERATION_COUNT)
            .map(|slice| truncate_field(String::from_utf8_lossy(slice).into_owned()))
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
    entry_infos: Vec<RawEntryInfo>,
    index_length: u32,
    data_length: u32,
    data_start: u32,
    data_end: u32,
    region_tag: u32,
    region_index_length: u32,
    region_data_length: u32,
}

impl HeaderBlob {
    pub(crate) fn parse(data: &[u8]) -> Result<HeaderBlob> {
        let mut cursor = Cursor::new(data);
        let index_length = read_be_u32(&mut cursor)?;
        let data_length = read_be_u32(&mut cursor)?;
        let data_start = 8 + index_length * ENTRY_INFO_DISK_SIZE;
        let total_length = data_start + data_length;
        let data_end = data_start + data_length;
        if index_length < 1 {
            return Err(anyhow!("RPM header blob has no index entries"));
        }
        if total_length >= HEADER_MAX_BYTES as u32 {
            return Err(anyhow!(
                "RPM header blob too large: total={} index_length={} data_length={}",
                total_length,
                index_length,
                data_length
            ));
        }

        if index_length as usize > MAX_ITERATION_COUNT {
            return Err(anyhow!(
                "RPM header index_length {} exceeds MAX_ITERATION_COUNT {}",
                index_length,
                MAX_ITERATION_COUNT
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
            .to_native()?;

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
            if self.region_index_length < self.entry_infos.len() as u32 - 1 {
                let (extra_entries, dribble_length) = swab_region(
                    data,
                    self.entry_infos[region_index_length as usize..].to_vec(),
                    computed_length,
                    self.data_start,
                    self.data_end,
                )?;
                entries.extend(extra_entries);
                entries = entries
                    .into_iter()
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                computed_length = dribble_length;
            }
            computed_length += ENTRY_INFO_DISK_SIZE;
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
            .to_native()?;
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
        if !(entry.kind == TagType::Bin && entry.count == REGION_TAG_COUNT) {
            return Err(anyhow!("invalid RPM region tag header"));
        }
        if entry.offset < 0
            || is_out_of_range(
                self.data_length,
                u32::try_from(entry.offset + ENTRY_INFO_DISK_SIZE as i32)?,
            )
        {
            return Err(anyhow!("invalid RPM region tag offset"));
        }

        let region_end = self.data_start + positive_offset(entry.offset)?;
        let trailer = parse_entry_info_le(
            data.get(region_end as usize..(region_end + ENTRY_INFO_DISK_SIZE) as usize)
                .context("invalid RPM region trailer slice")?,
        )?;

        self.region_data_length = region_end + ENTRY_INFO_DISK_SIZE - self.data_start;
        if region_tag == RPMTAG_HEADERSIGNATURES && entry.tag == RPMTAG_HEADERIMAGE {
            entry.tag = RPMTAG_HEADERSIGNATURES;
        }
        if !(entry.tag == region_tag
            && entry.kind == TagType::Bin
            && entry.count == REGION_TAG_COUNT)
        {
            return Err(anyhow!("invalid RPM region trailer header"));
        }

        let mut trailer = trailer.to_native()?;
        trailer.offset = -trailer.offset;
        self.region_index_length = positive_offset(trailer.offset)? / REGION_TAG_COUNT;
        if trailer.offset % REGION_TAG_COUNT as i32 != 0
            || is_out_of_range(self.index_length, self.region_index_length)
            || is_out_of_range(self.data_length, self.region_data_length)
        {
            return Err(anyhow!("invalid RPM region size metadata"));
        }
        self.region_tag = region_tag;
        Ok(())
    }

    fn verify_entries(&self, data: &[u8]) -> Result<()> {
        let mut end: u32 = 0;
        let entry_offset = usize::from(self.region_tag != 0);
        for entry in self.entry_infos[entry_offset..]
            .iter()
            .take(MAX_ITERATION_COUNT)
        {
            let info = entry.to_native()?;
            let kind = info.kind;
            let offset_u32 = positive_offset(info.offset)
                .map_err(|_| anyhow!("RPM header entry offset out of range"))?;
            if end > offset_u32 {
                return Err(anyhow!("RPM header entry offsets are not sorted"));
            }
            if is_reserved_tag(info.tag) {
                return Err(anyhow!("invalid RPM header tag {}", info.tag));
            }
            if is_misaligned(kind, info.offset) {
                return Err(anyhow!(
                    "misaligned RPM header entry offset {}",
                    info.offset
                ));
            }
            if is_out_of_range(self.data_length, offset_u32) {
                return Err(anyhow!("RPM header entry offset out of range"));
            }

            let length = compute_data_length(
                data,
                kind,
                info.count,
                self.data_start + offset_u32,
                self.data_end,
            );
            let length = match length {
                Some(l) => l,
                None => return Err(anyhow!("invalid RPM header entry length")),
            };
            end = offset_u32 + length as u32;
            if is_out_of_range(self.data_length, end) {
                return Err(anyhow!("invalid RPM header entry length"));
            }
        }

        Ok(())
    }
}

fn swab_region(
    data: &[u8],
    entry_infos: Vec<RawEntryInfo>,
    mut running_length: u32,
    data_start: u32,
    data_end: u32,
) -> Result<(Vec<IndexEntry>, u32)> {
    let mut entries = Vec::new();
    for (index, entry_info) in entry_infos.iter().enumerate().take(MAX_ITERATION_COUNT) {
        let info = entry_info.to_native()?;
        let kind = info.kind;
        let start = data_start + positive_offset(info.offset)?;
        if start >= data_end {
            return Err(anyhow!("RPM entry data offset is outside payload"));
        }

        let length = if index < entry_infos.len() - 1 && kind.is_variable_length() {
            let next_offset = entry_infos[index + 1].to_native()?.offset;
            positive_offset(next_offset - info.offset)? as usize
        } else {
            compute_data_length(data, kind, info.count, start, data_end)
                .ok_or_else(|| anyhow!("RPM entry data length is invalid"))?
        };

        let end = start as usize + length;
        entries.push(IndexEntry {
            info: info.clone(),
            data: data[start as usize..end].to_vec(),
        });
        running_length += length as u32 + alignment_padding(kind, running_length);
    }

    Ok((entries, running_length))
}

fn compute_data_length(
    data: &[u8],
    kind: TagType,
    count: u32,
    start: u32,
    data_end: u32,
) -> Option<usize> {
    match kind {
        TagType::String if count != 1 => None,
        TagType::String => string_tag_length(data, 1, start, data_end),
        TagType::StringArray | TagType::I18nString => {
            string_tag_length(data, count, start, data_end)
        }
        _ => {
            let size = kind.element_size().unwrap_or(0) * count;
            if start + size > data_end {
                None
            } else {
                Some(size as usize)
            }
        }
    }
}

fn string_tag_length(data: &[u8], count: u32, start: u32, data_end: u32) -> Option<usize> {
    if start >= data_end {
        return None;
    }
    let mut length: usize = 0;
    for _ in 0..count {
        let offset = start as usize + length;
        if offset > data.len() {
            return None;
        }
        let position = data[offset..data_end as usize]
            .iter()
            .position(|&byte| byte == 0)?;
        length += position + 1;
    }
    Some(length)
}

fn alignment_padding(kind: TagType, current_length: u32) -> u32 {
    let align = kind.alignment();
    if align <= 1 {
        return 0;
    }
    let diff = align - (current_length % align);
    if diff == align { 0 } else { diff }
}

fn is_out_of_range(length: u32, offset: u32) -> bool {
    offset > length
}

fn positive_offset(offset: i32) -> Result<u32> {
    u32::try_from(offset).map_err(|_| anyhow!("RPM header offset is negative: {offset}"))
}

fn is_reserved_tag(tag: u32) -> bool {
    tag < HEADER_I18NTABLE
}

fn is_misaligned(kind: TagType, offset: i32) -> bool {
    let align = kind.alignment() as i32;
    offset & (align - 1) != 0
}

fn read_be_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32> {
    let mut bytes = [0_u8; 4];
    std::io::Read::read_exact(cursor, &mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}

fn read_entry_info_le(cursor: &mut Cursor<&[u8]>) -> Result<RawEntryInfo> {
    let mut bytes = [0_u8; ENTRY_INFO_DISK_SIZE as usize];
    std::io::Read::read_exact(cursor, &mut bytes)?;
    parse_entry_info_le(&bytes)
}

fn parse_entry_info_le(data: &[u8]) -> Result<RawEntryInfo> {
    let mut offset = 0;
    Ok(RawEntryInfo {
        tag: read_u32_le(data, &mut offset)?,
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
