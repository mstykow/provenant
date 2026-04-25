// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use crate::models::FileInfo;
#[cfg(any(test, feature = "golden-tests"))]
use crate::models::Package;

use super::FileIx;
use super::package_file_index::PackageFileIndex;

#[derive(Clone, Copy)]
pub(super) struct FileClassification {
    pub(super) is_legal: bool,
    pub(super) is_manifest: bool,
    pub(super) is_readme: bool,
    pub(super) is_top_level: bool,
    pub(super) is_key_file: bool,
    pub(super) is_community: bool,
}

const LEGAL_STARTS_ENDS: &[&str] = &[
    "copying",
    "copyright",
    "copyrights",
    "copyleft",
    "notice",
    "license",
    "licenses",
    "licence",
    "licences",
    "licensing",
    "licencing",
    "legal",
    "eula",
    "agreement",
    "patent",
    "patents",
];

const MANIFEST_ENDS: &[&str] = &[
    ".about",
    "/bower.json",
    "/project.clj",
    ".podspec",
    "/composer.json",
    "/description",
    "/elm-package.json",
    "/+compact_manifest",
    "+manifest",
    ".gemspec",
    "/metadata",
    "/metadata.gz-extract",
    "/build.gradle",
    ".cabal",
    "/haxelib.json",
    "/package.json",
    ".nuspec",
    ".pod",
    "/meta.yml",
    "/dist.ini",
    "/pipfile",
    "/setup.cfg",
    "/setup.py",
    "/pkg-info",
    "/pyproject.toml",
    ".spec",
    "/cargo.toml",
    ".spdx",
    "/dependencies",
    "debian/copyright",
    "meta-inf/manifest.mf",
];

pub(super) fn apply_file_classification(
    files: &mut [FileInfo],
    package_file_index: &PackageFileIndex,
) {
    for idx in 0..files.len() {
        let classification = package_file_index.classify_file(files, FileIx(idx));
        let file = &mut files[idx];
        file.is_legal = classification.is_legal;
        file.is_manifest = classification.is_manifest;
        file.is_readme = classification.is_readme;
        file.is_top_level = classification.is_top_level;
        file.is_key_file = classification.is_key_file;
        file.is_community = classification.is_community;
    }
}

#[cfg(any(test, feature = "golden-tests"))]
pub(super) fn classify_key_files(files: &mut [FileInfo], packages: &[Package]) {
    let package_file_index = PackageFileIndex::build(files, packages);
    apply_file_classification(files, &package_file_index);
}

fn name_or_base_name_matches(file: &FileInfo, patterns: &[&str]) -> bool {
    let name = file.name.to_ascii_lowercase();
    let base_name = file.base_name.to_ascii_lowercase();

    patterns.iter().any(|pattern| {
        name.starts_with(pattern)
            || name.ends_with(pattern)
            || base_name.starts_with(pattern)
            || base_name.ends_with(pattern)
    })
}

pub(super) fn is_legal_file(file: &FileInfo) -> bool {
    name_or_base_name_matches(file, LEGAL_STARTS_ENDS)
}

pub(super) fn is_manifest_file(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    MANIFEST_ENDS.iter().any(|ending| lowered.ends_with(ending))
}

pub(super) fn is_readme_file(file: &FileInfo) -> bool {
    name_or_base_name_matches(file, &["readme"])
}

pub(super) fn is_community_file(file: &FileInfo) -> bool {
    let clean = |s: &str| s.replace(['_', '-'], "").to_ascii_lowercase();
    let candidates = [clean(&file.name), clean(&file.base_name)];
    [
        "changelog",
        "roadmap",
        "contributing",
        "codeofconduct",
        "authors",
        "security",
        "funding",
    ]
    .iter()
    .any(|prefix| {
        candidates
            .iter()
            .any(|candidate| candidate.starts_with(prefix) || candidate.ends_with(prefix))
    })
}
