use anyhow::{Result, anyhow};

use crate::parser_warn as warn;
use crate::parsers::utils::{MAX_ITERATION_COUNT, truncate_field};

use super::entry::IndexEntry;
use super::tags::{
    RPMTAG_ARCH, RPMTAG_BASENAMES, RPMTAG_DIRINDEXES, RPMTAG_DIRNAMES, RPMTAG_DISTRIBUTION,
    RPMTAG_EPOCH, RPMTAG_FILENAMES, RPMTAG_LICENSE, RPMTAG_NAME, RPMTAG_PLATFORM,
    RPMTAG_PROVIDENAME, RPMTAG_RELEASE, RPMTAG_REQUIRENAME, RPMTAG_SIZE, RPMTAG_SOURCERPM,
    RPMTAG_VENDOR, RPMTAG_VERSION, TagType,
};

#[derive(Debug, Default, Clone)]
pub(crate) struct InstalledRpmPackage {
    pub(crate) epoch: u32,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) release: String,
    pub(crate) arch: String,
    pub(crate) source_rpm: String,
    pub(crate) size: u32,
    pub(crate) license: String,
    pub(crate) vendor: String,
    pub(crate) distribution: String,
    pub(crate) platform: String,
    pub(crate) base_names: Vec<String>,
    pub(crate) dir_indexes: Vec<u32>,
    pub(crate) dir_names: Vec<String>,
    pub(crate) file_names: Vec<String>,
    pub(crate) provides: Vec<String>,
    pub(crate) requires: Vec<String>,
}

pub(crate) fn parse_installed_rpm_package(entries: Vec<IndexEntry>) -> Result<InstalledRpmPackage> {
    let mut package = InstalledRpmPackage::default();
    for (i, entry) in entries.into_iter().enumerate() {
        if i >= MAX_ITERATION_COUNT {
            warn!(
                "RPM entry iteration exceeded MAX_ITERATION_COUNT ({}), stopping",
                MAX_ITERATION_COUNT
            );
            break;
        }
        match entry.info.tag {
            RPMTAG_DIRINDEXES => {
                ensure_kind(&entry, TagType::Int32, "dir indexes")?;
                package.dir_indexes = entry.read_u32_array()?;
            }
            RPMTAG_DIRNAMES => {
                ensure_kind(&entry, TagType::StringArray, "dir names")?;
                package.dir_names = entry
                    .read_string_array()?
                    .into_iter()
                    .map(truncate_field)
                    .collect();
            }
            RPMTAG_BASENAMES => {
                ensure_kind(&entry, TagType::StringArray, "base names")?;
                package.base_names = entry
                    .read_string_array()?
                    .into_iter()
                    .map(truncate_field)
                    .collect();
            }
            RPMTAG_FILENAMES => {
                ensure_kind(&entry, TagType::StringArray, "file names")?;
                package.file_names = entry
                    .read_string_array()?
                    .into_iter()
                    .map(truncate_field)
                    .collect();
            }
            RPMTAG_NAME => {
                ensure_kind(&entry, TagType::String, "name")?;
                package.name = truncate_field(entry.read_string()?);
            }
            RPMTAG_EPOCH => {
                ensure_kind(&entry, TagType::Int32, "epoch")?;
                package.epoch = entry.read_u32()?;
            }
            RPMTAG_VERSION => {
                ensure_kind(&entry, TagType::String, "version")?;
                package.version = truncate_field(entry.read_string()?);
            }
            RPMTAG_RELEASE => {
                ensure_kind(&entry, TagType::String, "release")?;
                package.release = truncate_field(entry.read_string()?);
            }
            RPMTAG_ARCH => {
                ensure_kind(&entry, TagType::String, "arch")?;
                package.arch = truncate_field(entry.read_string()?);
            }
            RPMTAG_SOURCERPM => {
                ensure_kind(&entry, TagType::String, "source rpm")?;
                package.source_rpm = normalize_none(truncate_field(entry.read_string()?));
            }
            RPMTAG_LICENSE => {
                ensure_kind(&entry, TagType::String, "license")?;
                package.license = normalize_none(truncate_field(entry.read_string()?));
            }
            RPMTAG_VENDOR => {
                ensure_kind(&entry, TagType::String, "vendor")?;
                package.vendor = normalize_none(truncate_field(entry.read_string()?));
            }
            RPMTAG_DISTRIBUTION => {
                ensure_kind(&entry, TagType::String, "distribution")?;
                package.distribution = normalize_none(truncate_field(entry.read_string()?));
            }
            RPMTAG_PLATFORM => {
                ensure_kind(&entry, TagType::String, "platform")?;
                package.platform = normalize_none(entry.read_string()?);
            }
            RPMTAG_SIZE => {
                ensure_kind(&entry, TagType::Int32, "size")?;
                package.size = entry.read_u32()?;
            }
            RPMTAG_PROVIDENAME => {
                ensure_kind(&entry, TagType::StringArray, "provide names")?;
                package.provides = entry.read_string_array()?;
            }
            RPMTAG_REQUIRENAME => {
                ensure_kind(&entry, TagType::StringArray, "require names")?;
                package.requires = entry.read_string_array()?;
            }
            _ => {}
        }
    }

    Ok(package)
}

fn ensure_kind(entry: &IndexEntry, expected: TagType, label: &str) -> Result<()> {
    if entry.info.kind != expected {
        return Err(anyhow!(
            "invalid RPM tag type for {}: expected={:?}, actual={:?}",
            label,
            expected,
            entry.info.kind
        ));
    }
    Ok(())
}

fn normalize_none(value: String) -> String {
    if value == "(none)" {
        String::new()
    } else {
        value
    }
}
