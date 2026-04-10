mod datasource_id;
mod dependency_uid;
mod digest;
pub(crate) mod file_info;
mod line_number;
mod output;
mod package_type;
mod package_uid;

pub use datasource_id::DatasourceId;
pub use dependency_uid::DependencyUid;
pub use digest::{GitSha1, Md5Digest, Sha1Digest, Sha256Digest, Sha512Digest};
pub use file_info::{
    Author, Copyright, Dependency, FileInfo, FileInfoBuilder, FileReference, FileType, Holder,
    LicenseDetection, LicensePolicyEntry, Match, OutputEmail, OutputURL, Package, PackageData,
    Party, ResolvedPackage, TopLevelDependency,
};
pub use line_number::LineNumber;
pub use package_type::PackageType;
pub use package_uid::PackageUid;

pub use output::{
    ExtraData, FacetTallies, Header, LicenseClarityScore, LicenseReference, LicenseRuleReference,
    OUTPUT_FORMAT_VERSION, Output, SPDX_LICENSE_LIST_VERSION, Summary, SystemEnvironment,
    TOOL_NAME, Tallies, TallyEntry, TopLevelLicenseDetection,
};
