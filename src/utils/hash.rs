use md5::{Digest as Md5Digest, Md5};
use sha1::Sha1;
use sha2::Sha256;

use crate::models::{GitSha1, Md5Digest as Md5DigestType, Sha1Digest, Sha256Digest};

pub fn calculate_sha1(content: &[u8]) -> Sha1Digest {
    let digest = Sha1::digest(content);
    Sha1Digest::from_bytes(digest.into())
}

pub fn calculate_md5(content: &[u8]) -> Md5DigestType {
    let digest = Md5::digest(content);
    Md5DigestType::from_bytes(digest.into())
}

pub fn calculate_sha256(content: &[u8]) -> Sha256Digest {
    let digest = Sha256::digest(content);
    Sha256Digest::from_bytes(digest.into())
}

pub fn calculate_sha1_git(content: &[u8]) -> GitSha1 {
    let mut payload = Vec::with_capacity(content.len() + 32);
    payload.extend_from_slice(format!("blob {}\0", content.len()).as_bytes());
    payload.extend_from_slice(content);
    let digest = Sha1::digest(&payload);
    GitSha1::from_bytes(digest.into())
}
