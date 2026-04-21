// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

mod datasource_id;
mod dependency_uid;
mod diagnostic;
mod digest;
pub(crate) mod file_info;
mod line_number;
mod match_score;
mod output;
mod package_type;
mod package_uid;

pub use datasource_id::DatasourceId;
pub use dependency_uid::DependencyUid;
pub use diagnostic::{
    DiagnosticSeverity, ScanDiagnostic, diagnostics_from_legacy_scan_errors,
    is_legacy_warning_message,
};
pub use digest::{GitSha1, Md5Digest, Sha1Digest, Sha256Digest, Sha512Digest};
pub use file_info::{
    Author, Copyright, Dependency, FileInfo, FileInfoBuilder, FileReference, FileType, Holder,
    LicenseDetection, LicensePolicyEntry, Match, OutputEmail, OutputURL, Package, PackageData,
    Party, ResolvedPackage, TopLevelDependency,
};
pub use line_number::LineNumber;
pub use match_score::MatchScore;
pub use package_type::PackageType;
pub use package_uid::PackageUid;

pub use output::{
    ExtraData, FacetTallies, HEADER_NOTICE, Header, LicenseClarityScore, LicenseIndexProvenance,
    LicenseReference, LicenseRuleReference, OUTPUT_FORMAT_VERSION, Output, Summary,
    SystemEnvironment, TOOL_NAME, Tallies, TallyEntry, TopLevelLicenseDetection,
};
