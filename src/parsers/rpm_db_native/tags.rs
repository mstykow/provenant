pub(crate) const RPMTAG_HEADERIMAGE: u32 = 61;
pub(crate) const RPMTAG_HEADERSIGNATURES: u32 = 62;
pub(crate) const RPMTAG_HEADERIMMUTABLE: u32 = 63;
pub(crate) const HEADER_I18NTABLE: u32 = 100;
pub(crate) const RPMTAG_HEADERI18NTABLE: u32 = HEADER_I18NTABLE;

pub(crate) const RPMTAG_NAME: u32 = 1000;
pub(crate) const RPMTAG_VERSION: u32 = 1001;
pub(crate) const RPMTAG_RELEASE: u32 = 1002;
pub(crate) const RPMTAG_EPOCH: u32 = 1003;
pub(crate) const RPMTAG_DISTRIBUTION: u32 = 1010;
pub(crate) const RPMTAG_VENDOR: u32 = 1011;
pub(crate) const RPMTAG_LICENSE: u32 = 1014;
pub(crate) const RPMTAG_ARCH: u32 = 1022;
pub(crate) const RPMTAG_SOURCERPM: u32 = 1044;
pub(crate) const RPMTAG_PROVIDENAME: u32 = 1047;
pub(crate) const RPMTAG_REQUIRENAME: u32 = 1049;
pub(crate) const RPMTAG_DIRINDEXES: u32 = 1116;
pub(crate) const RPMTAG_BASENAMES: u32 = 1117;
pub(crate) const RPMTAG_DIRNAMES: u32 = 1118;
pub(crate) const RPMTAG_PLATFORM: u32 = 1132;
pub(crate) const RPMTAG_SIZE: u32 = 1009;
pub(crate) const RPMTAG_FILENAMES: u32 = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TagType {
    Null,
    Char,
    Int8,
    Int16,
    Int32,
    Int64,
    String,
    Bin,
    StringArray,
    I18nString,
}

impl TagType {
    pub(crate) fn from_raw(code: u32) -> anyhow::Result<Self> {
        match code {
            0 => Ok(Self::Null),
            1 => Ok(Self::Char),
            2 => Ok(Self::Int8),
            3 => Ok(Self::Int16),
            4 => Ok(Self::Int32),
            5 => Ok(Self::Int64),
            6 => Ok(Self::String),
            7 => Ok(Self::Bin),
            8 => Ok(Self::StringArray),
            9 => Ok(Self::I18nString),
            _ => Err(anyhow::anyhow!("invalid RPM tag type: {code}")),
        }
    }

    pub(crate) fn element_size(&self) -> Option<u32> {
        match self {
            Self::Null | Self::Char | Self::Int8 | Self::Bin => Some(1),
            Self::Int16 => Some(2),
            Self::Int32 => Some(4),
            Self::Int64 => Some(8),
            Self::String | Self::StringArray | Self::I18nString => None,
        }
    }

    pub(crate) fn alignment(&self) -> u32 {
        match self {
            Self::Int16 => 2,
            Self::Int32 => 4,
            Self::Int64 => 8,
            _ => 1,
        }
    }

    pub(crate) fn is_variable_length(&self) -> bool {
        self.element_size().is_none()
    }
}
