use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::resolved_package::OutputResolvedPackage;
use super::serde_helpers::serialize_optional_map_as_object;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputTopLevelDependency {
    pub purl: Option<String>,
    pub extracted_requirement: Option<String>,
    pub scope: Option<String>,
    pub is_runtime: Option<bool>,
    pub is_optional: Option<bool>,
    pub is_pinned: Option<bool>,
    pub is_direct: Option<bool>,
    pub resolved_package: Option<Box<OutputResolvedPackage>>,
    #[serde(default, serialize_with = "serialize_optional_map_as_object")]
    pub extra_data: Option<HashMap<String, serde_json::Value>>,
    pub dependency_uid: String,
    pub for_package_uid: Option<String>,
    pub datafile_path: String,
    pub datasource_id: crate::models::DatasourceId,
    pub namespace: Option<String>,
}

impl From<&crate::models::TopLevelDependency> for OutputTopLevelDependency {
    fn from(value: &crate::models::TopLevelDependency) -> Self {
        Self {
            purl: value.purl.clone(),
            extracted_requirement: value.extracted_requirement.clone(),
            scope: value.scope.clone(),
            is_runtime: value.is_runtime,
            is_optional: value.is_optional,
            is_pinned: value.is_pinned,
            is_direct: value.is_direct,
            resolved_package: value
                .resolved_package
                .as_ref()
                .map(|rp| Box::new(OutputResolvedPackage::from(rp.as_ref()))),
            extra_data: value.extra_data.clone(),
            dependency_uid: value.dependency_uid.to_string(),
            for_package_uid: value.for_package_uid.as_ref().map(|uid| uid.to_string()),
            datafile_path: value.datafile_path.clone(),
            datasource_id: value.datasource_id,
            namespace: value.namespace.clone(),
        }
    }
}

impl TryFrom<&OutputTopLevelDependency> for crate::models::TopLevelDependency {
    type Error = String;
    fn try_from(value: &OutputTopLevelDependency) -> Result<Self, Self::Error> {
        let resolved_package = value
            .resolved_package
            .as_ref()
            .map(|rp| crate::models::ResolvedPackage::try_from(rp.as_ref()).map(Box::new))
            .transpose()?;
        Ok(Self {
            purl: value.purl.clone(),
            extracted_requirement: value.extracted_requirement.clone(),
            scope: value.scope.clone(),
            is_runtime: value.is_runtime,
            is_optional: value.is_optional,
            is_pinned: value.is_pinned,
            is_direct: value.is_direct,
            resolved_package,
            extra_data: value.extra_data.clone(),
            dependency_uid: crate::models::DependencyUid::from_raw(value.dependency_uid.clone()),
            for_package_uid: value
                .for_package_uid
                .as_ref()
                .map(|s| crate::models::PackageUid::from_raw(s.clone())),
            datafile_path: value.datafile_path.clone(),
            datasource_id: value.datasource_id,
            namespace: value.namespace.clone(),
        })
    }
}
