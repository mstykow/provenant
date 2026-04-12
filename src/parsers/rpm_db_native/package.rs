use anyhow::{Result, anyhow};

use super::entry::IndexEntry;
use super::tags::{
    RPM_INT32_TYPE, RPM_STRING_ARRAY_TYPE, RPM_STRING_TYPE, RPMTAG_ARCH, RPMTAG_BASENAMES,
    RPMTAG_DIRINDEXES, RPMTAG_DIRNAMES, RPMTAG_DISTRIBUTION, RPMTAG_EPOCH, RPMTAG_FILENAMES,
    RPMTAG_LICENSE, RPMTAG_NAME, RPMTAG_PLATFORM, RPMTAG_PROVIDENAME, RPMTAG_RELEASE,
    RPMTAG_REQUIRENAME, RPMTAG_SIZE, RPMTAG_SOURCERPM, RPMTAG_VENDOR, RPMTAG_VERSION,
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
    for entry in entries {
        match entry.info.tag {
            RPMTAG_DIRINDEXES => {
                ensure_kind(&entry, RPM_INT32_TYPE, "dir indexes")?;
                package.dir_indexes = entry.read_u32_array()?;
            }
            RPMTAG_DIRNAMES => {
                ensure_kind(&entry, RPM_STRING_ARRAY_TYPE, "dir names")?;
                package.dir_names = entry.read_string_array()?;
            }
            RPMTAG_BASENAMES => {
                ensure_kind(&entry, RPM_STRING_ARRAY_TYPE, "base names")?;
                package.base_names = entry.read_string_array()?;
            }
            RPMTAG_FILENAMES => {
                ensure_kind(&entry, RPM_STRING_ARRAY_TYPE, "file names")?;
                package.file_names = entry.read_string_array()?;
            }
            RPMTAG_NAME => {
                ensure_kind(&entry, RPM_STRING_TYPE, "name")?;
                package.name = entry.read_string()?;
            }
            RPMTAG_EPOCH => {
                ensure_kind(&entry, RPM_INT32_TYPE, "epoch")?;
                package.epoch = entry.read_u32()?;
            }
            RPMTAG_VERSION => {
                ensure_kind(&entry, RPM_STRING_TYPE, "version")?;
                package.version = entry.read_string()?;
            }
            RPMTAG_RELEASE => {
                ensure_kind(&entry, RPM_STRING_TYPE, "release")?;
                package.release = entry.read_string()?;
            }
            RPMTAG_ARCH => {
                ensure_kind(&entry, RPM_STRING_TYPE, "arch")?;
                package.arch = entry.read_string()?;
            }
            RPMTAG_SOURCERPM => {
                ensure_kind(&entry, RPM_STRING_TYPE, "source rpm")?;
                package.source_rpm = normalize_none(entry.read_string()?);
            }
            RPMTAG_LICENSE => {
                ensure_kind(&entry, RPM_STRING_TYPE, "license")?;
                package.license = normalize_none(entry.read_string()?);
            }
            RPMTAG_VENDOR => {
                ensure_kind(&entry, RPM_STRING_TYPE, "vendor")?;
                package.vendor = normalize_none(entry.read_string()?);
            }
            RPMTAG_DISTRIBUTION => {
                ensure_kind(&entry, RPM_STRING_TYPE, "distribution")?;
                package.distribution = normalize_none(entry.read_string()?);
            }
            RPMTAG_PLATFORM => {
                ensure_kind(&entry, RPM_STRING_TYPE, "platform")?;
                package.platform = normalize_none(entry.read_string()?);
            }
            RPMTAG_SIZE => {
                ensure_kind(&entry, RPM_INT32_TYPE, "size")?;
                package.size = entry.read_u32()?;
            }
            RPMTAG_PROVIDENAME => {
                ensure_kind(&entry, RPM_STRING_ARRAY_TYPE, "provide names")?;
                package.provides = entry.read_string_array()?;
            }
            RPMTAG_REQUIRENAME => {
                ensure_kind(&entry, RPM_STRING_ARRAY_TYPE, "require names")?;
                package.requires = entry.read_string_array()?;
            }
            _ => {}
        }
    }

    Ok(package)
}

fn ensure_kind(entry: &IndexEntry, expected: u32, label: &str) -> Result<()> {
    if entry.info.kind != expected {
        return Err(anyhow!(
            "invalid RPM tag type for {}: expected={}, actual={}",
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
