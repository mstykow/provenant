use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputParty {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

impl From<&crate::models::Party> for OutputParty {
    fn from(value: &crate::models::Party) -> Self {
        Self {
            r#type: value.r#type.clone(),
            role: value.role.clone(),
            name: value.name.clone(),
            email: value.email.clone(),
            url: value.url.clone(),
            organization: value.organization.clone(),
            organization_url: value.organization_url.clone(),
            timezone: value.timezone.clone(),
        }
    }
}

impl TryFrom<&OutputParty> for crate::models::Party {
    type Error = String;
    fn try_from(value: &OutputParty) -> Result<Self, Self::Error> {
        Ok(Self {
            r#type: value.r#type.clone(),
            role: value.role.clone(),
            name: value.name.clone(),
            email: value.email.clone(),
            url: value.url.clone(),
            organization: value.organization.clone(),
            organization_url: value.organization_url.clone(),
            timezone: value.timezone.clone(),
        })
    }
}
