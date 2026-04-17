//! Parser for Debian package metadata files.
//!
//! Extracts package metadata from Debian package management files using RFC 822
//! format parsing for control files and installed package databases.
//!
//! # Supported Formats
//! - `debian/control` (Source package control files - multi-paragraph)
//! - `/var/lib/dpkg/status` (Installed package database - multi-paragraph)
//! - `/var/lib/dpkg/status.d/*` (Distroless installed packages)
//! - `*.dsc` (Debian source control files)
//! - `*.orig.tar.*` (Original upstream tarballs)
//! - `*.debian.tar.*` (Debian packaging tarballs)
//! - `/var/lib/dpkg/info/*.list` (Installed file lists)
//! - `/var/lib/dpkg/info/*.md5sums` (Installed file checksums)
//! - `debian/copyright` (Copyright/license declarations)
//! - `*.deb` (Debian binary package archives)
//! - `control` (extracted from .deb archives)
//! - `md5sums` (extracted from .deb archives)
//!
//! # Key Features
//! - RFC 822 format parsing for control files
//! - Dependency extraction with scope tracking (Depends, Build-Depends, etc.)
//! - Debian vs Ubuntu namespace detection from version and maintainer fields
//! - Multi-paragraph record parsing for package databases
//! - License and copyright information extraction
//! - Package URL (purl) generation with namespace
//!
//! # Implementation Notes
//! - Uses RFC 822 parser from `crate::parsers::rfc822` module
//! - Multi-paragraph records separated by blank lines
//! - Graceful error handling with `warn!()` logs

mod control;
mod copyright;
mod deb;
mod dsc;
mod file_list;
mod tarball;
mod utils;

#[cfg(test)]
mod deb_extra_test;
#[cfg(test)]
mod scan_test;

pub use self::control::{
    DebianControlParser, DebianDistrolessInstalledParser, DebianInstalledParser,
};
pub use self::copyright::DebianCopyrightParser;
pub use self::deb::{
    DebianControlInExtractedDebParser, DebianDebParser, DebianMd5sumInPackageParser,
};
pub use self::dsc::DebianDscParser;
pub use self::file_list::{DebianInstalledListParser, DebianInstalledMd5sumsParser};
pub use self::tarball::{DebianDebianTarParser, DebianOrigTarParser};

use std::sync::LazyLock;

use crate::models::{DatasourceId, PackageData, PackageType};
use regex::Regex;

const PACKAGE_TYPE: PackageType = PackageType::Deb;

const MAX_ARCHIVE_SIZE: u64 = 1024 * 1024 * 1024;
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: usize = 100;

static DEP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\s*([a-zA-Z0-9][a-zA-Z0-9.+\-]+)\s*(?:\(([<>=!]+)\s*([^)]+)\))?\s*(?:\[.*\])?\s*$",
    )
    .expect("compile-time constant dependency regex")
});

fn default_package_data(datasource_id: DatasourceId) -> PackageData {
    PackageData {
        package_type: Some(PACKAGE_TYPE),
        datasource_id: Some(datasource_id),
        ..Default::default()
    }
}

macro_rules! read_or_default {
    ($path:expr, $msg:expr, $dsid:expr) => {
        match crate::parsers::utils::read_file_to_string($path, None) {
            Ok(c) => c,
            Err(e) => {
                crate::parser_warn!("Failed to read {} at {:?}: {}", $msg, $path, e);
                return vec![default_package_data($dsid)];
            }
        }
    };
}

use read_or_default;

use std::fmt;

enum Namespace {
    Debian,
    Ubuntu,
}

impl Namespace {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Debian => "debian",
            Self::Ubuntu => "ubuntu",
        }
    }

    fn version_clues(&self) -> &[&str] {
        match self {
            Self::Ubuntu => &["ubuntu"],
            Self::Debian => &["deb"],
        }
    }

    fn maintainer_clues(&self) -> &[&str] {
        match self {
            Self::Ubuntu => &["lists.ubuntu.com", "@canonical.com"],
            Self::Debian => &[
                "packages.debian.org",
                "lists.debian.org",
                "lists.alioth.debian.org",
                "@debian.org",
                "debian-init-diversity@",
            ],
        }
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

const NAMESPACE_PRIORITY: &[Namespace] = &[Namespace::Ubuntu, Namespace::Debian];

enum DepField {
    Depends,
    PreDepends,
    Recommends,
    Suggests,
    Breaks,
    Conflicts,
    Replaces,
    Provides,
    BuildDepends,
    BuildDependsIndep,
    BuildConflicts,
}

impl DepField {
    fn field(&self) -> &'static str {
        match self {
            Self::Depends => "depends",
            Self::PreDepends => "pre-depends",
            Self::Recommends => "recommends",
            Self::Suggests => "suggests",
            Self::Breaks => "breaks",
            Self::Conflicts => "conflicts",
            Self::Replaces => "replaces",
            Self::Provides => "provides",
            Self::BuildDepends => "build-depends",
            Self::BuildDependsIndep => "build-depends-indep",
            Self::BuildConflicts => "build-conflicts",
        }
    }

    fn scope(&self) -> &'static str {
        self.field()
    }

    fn is_runtime(&self) -> bool {
        matches!(
            self,
            Self::Depends | Self::PreDepends | Self::Recommends | Self::Suggests
        )
    }

    fn is_optional(&self) -> bool {
        matches!(self, Self::Recommends | Self::Suggests)
    }
}

const DEP_FIELDS: &[DepField] = &[
    DepField::Depends,
    DepField::PreDepends,
    DepField::Recommends,
    DepField::Suggests,
    DepField::Breaks,
    DepField::Conflicts,
    DepField::Replaces,
    DepField::Provides,
    DepField::BuildDepends,
    DepField::BuildDependsIndep,
    DepField::BuildConflicts,
];

const IGNORED_ROOT_DIRS: &[&str] = &["/.", "/bin", "/etc", "/lib", "/sbin", "/usr", "/var"];
