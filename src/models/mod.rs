mod datasource_id;
pub(crate) mod file_info;
mod output;
mod package_type;

pub use datasource_id::DatasourceId;
pub use file_info::{
    Author, Copyright, Dependency, FileInfo, FileInfoBuilder, FileReference, FileType, Holder,
    LicenseDetection, LicensePolicyEntry, Match, OutputEmail, OutputURL, Package, PackageData,
    Party, ResolvedPackage, TopLevelDependency,
};
pub use package_type::PackageType;

#[cfg(test)]
pub use file_info::build_package_uid;
pub use output::{
    ExtraData, FacetTallies, Header, LicenseClarityScore, LicenseReference, LicenseRuleReference,
    OUTPUT_FORMAT_VERSION, Output, Summary, SystemEnvironment, Tallies, TallyEntry,
    TopLevelLicenseDetection,
};
