use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! define_digest {
    (
        $(#[$meta:meta])*
        $name:ident, $byte_len:literal
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; $byte_len]);

        impl $name {
            pub const EMPTY: Self = Self([0u8; $byte_len]);

            pub const fn from_bytes(bytes: [u8; $byte_len]) -> Self {
                Self(bytes)
            }

            pub fn from_hex(s: &str) -> Result<Self, ParseDigestError> {
                let bytes = hex::decode(s).map_err(|_| ParseDigestError::InvalidHex)?;
                let array: [u8; $byte_len] = bytes
                    .try_into()
                    .map_err(|_: Vec<u8>| ParseDigestError::InvalidLength)?;
                Ok(Self(array))
            }

            pub fn as_bytes(&self) -> &[u8; $byte_len] {
                &self.0
            }

            pub fn as_hex(&self) -> String {
                hex::encode(self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name))
                    .field(&self.as_hex())
                    .finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.as_hex())
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.as_hex())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                Self::from_hex(&s).map_err(serde::de::Error::custom)
            }
        }
    };
}

define_digest!(
    /// SHA-1 digest (20 bytes / 40 hex characters).
    Sha1Digest,
    20
);

define_digest!(
    /// MD5 digest (16 bytes / 32 hex characters).
    Md5Digest,
    16
);

define_digest!(
    /// SHA-256 digest (32 bytes / 64 hex characters).
    Sha256Digest,
    32
);

define_digest!(
    /// SHA-512 digest (64 bytes / 128 hex characters).
    Sha512Digest,
    64
);

define_digest!(
    /// Git object SHA-1 digest (20 bytes / 40 hex characters).
    GitSha1,
    20
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseDigestError {
    InvalidHex,
    InvalidLength,
}

impl std::fmt::Display for ParseDigestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseDigestError::InvalidHex => write!(f, "invalid hex encoding"),
            ParseDigestError::InvalidLength => write!(f, "invalid digest length"),
        }
    }
}

impl std::error::Error for ParseDigestError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_roundtrip() {
        let hex = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
        let digest = Sha1Digest::from_hex(hex).unwrap();
        assert_eq!(digest.as_hex(), hex);
    }

    #[test]
    fn sha256_roundtrip() {
        let hex = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let digest = Sha256Digest::from_hex(hex).unwrap();
        assert_eq!(digest.as_hex(), hex);
    }

    #[test]
    fn sha512_roundtrip() {
        let hex = "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e";
        let digest = Sha512Digest::from_hex(hex).unwrap();
        assert_eq!(digest.as_hex(), hex);
    }

    #[test]
    fn md5_roundtrip() {
        let hex = "d41d8cd98f00b204e9800998ecf8427e";
        let digest = Md5Digest::from_hex(hex).unwrap();
        assert_eq!(digest.as_hex(), hex);
    }

    #[test]
    fn git_sha1_roundtrip() {
        let hex = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
        let digest = GitSha1::from_hex(hex).unwrap();
        assert_eq!(digest.as_hex(), hex);
    }

    #[test]
    fn invalid_hex_rejected() {
        assert!(Sha1Digest::from_hex("not-hex!").is_err());
    }

    #[test]
    fn invalid_length_rejected() {
        assert!(Sha1Digest::from_hex("abcd").is_err());
        assert!(Sha256Digest::from_hex("da39a3ee5e6b4b0d3255bfef95601890afd80709").is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let hex = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
        let digest = Sha1Digest::from_hex(hex).unwrap();
        let json = serde_json::to_string(&digest).unwrap();
        assert_eq!(json, format!("\"{}\"", hex));
        let back: Sha1Digest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, digest);
    }

    #[test]
    fn optional_serde_roundtrip() {
        let some: Option<Sha1Digest> =
            Some(Sha1Digest::from_hex("da39a3ee5e6b4b0d3255bfef95601890afd80709").unwrap());
        let json = serde_json::to_string(&some).unwrap();
        let back: Option<Sha1Digest> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, some);

        let none: Option<Sha1Digest> = None;
        let json = serde_json::to_string(&none).unwrap();
        let back: Option<Sha1Digest> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, none);
    }

    #[test]
    fn empty_constant_is_all_zeros() {
        assert_eq!(Sha1Digest::EMPTY.0, [0u8; 20]);
        assert_eq!(Md5Digest::EMPTY.0, [0u8; 16]);
        assert_eq!(Sha256Digest::EMPTY.0, [0u8; 32]);
        assert_eq!(Sha512Digest::EMPTY.0, [0u8; 64]);
        assert_eq!(GitSha1::EMPTY.0, [0u8; 20]);
    }

    #[test]
    fn display_shows_hex() {
        let hex = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
        let digest = Sha1Digest::from_hex(hex).unwrap();
        assert_eq!(format!("{}", digest), hex);
    }
}
