use std::path::PathBuf;

use crate::models::{DatasourceId, PackageType, Sha256Digest};

use super::PackageParser;
use super::hex_lock::HexLockParser;

#[test]
fn test_hex_mix_lock_is_match() {
    assert!(HexLockParser::is_match(&PathBuf::from("/tmp/mix.lock")));
    assert!(!HexLockParser::is_match(&PathBuf::from("/tmp/mix.exs")));
}

#[test]
fn test_parse_hex_mix_lock_basic() {
    let path = PathBuf::from("testdata/hex/basic/mix.lock");
    let package_data = HexLockParser::extract_first_package(&path);

    assert_eq!(package_data.package_type, Some(PackageType::Hex));
    assert_eq!(package_data.primary_language.as_deref(), Some("Elixir"));
    assert_eq!(package_data.datasource_id, Some(DatasourceId::HexMixLock));
    assert!(package_data.name.is_none());
    assert!(package_data.version.is_none());
    assert_eq!(package_data.dependencies.len(), 4);

    let plug = package_data
        .dependencies
        .iter()
        .find(|dep| dep.purl.as_deref() == Some("pkg:hex/plug@1.18.1"))
        .expect("expected plug dependency");
    assert_eq!(plug.extracted_requirement.as_deref(), Some("1.18.1"));
    assert_eq!(plug.scope.as_deref(), Some("dependencies"));
    assert_eq!(plug.is_direct, Some(false));
    assert_eq!(plug.is_runtime, Some(true));
    assert_eq!(plug.is_optional, Some(false));
    assert_eq!(plug.is_pinned, Some(true));

    let resolved = plug
        .resolved_package
        .as_ref()
        .expect("resolved package should exist");
    assert_eq!(resolved.name, "plug");
    assert_eq!(resolved.version, "1.18.1");
    assert_eq!(resolved.package_type, PackageType::Hex);
    assert_eq!(
        resolved.sha256,
        Some(
            Sha256Digest::from_hex(
                "5067f26f7745b7e31bc3368bc1a2b818b9779faa959b49c934c17730efc911cf"
            )
            .unwrap()
        )
    );
    let extra = resolved
        .extra_data
        .as_ref()
        .expect("extra_data should exist");
    assert_eq!(extra.get("repo"), Some(&serde_json::json!("hexpm")));
    assert_eq!(
        extra.get("outer_checksum"),
        Some(&serde_json::json!(
            "57a57db70df2b422b564437d2d33cf8d33cd16339c1edb190cd11b1a3a546cc2"
        ))
    );
    assert_eq!(extra.get("managers"), Some(&serde_json::json!(["mix"])));
    assert!(
        resolved
            .dependencies
            .iter()
            .any(|dep| dep.purl.as_deref() == Some("pkg:hex/mime"))
    );
    assert!(
        resolved
            .dependencies
            .iter()
            .any(|dep| dep.purl.as_deref() == Some("pkg:hex/plug_crypto"))
    );
}

#[test]
fn test_parse_hex_mix_lock_preserves_hex_package_aliases() {
    let path = PathBuf::from("testdata/hex/alias/mix.lock");
    let package_data = HexLockParser::extract_first_package(&path);

    let postgres = package_data
        .dependencies
        .iter()
        .find(|dep| dep.purl.as_deref() == Some("pkg:hex/postgrex@0.20.0"))
        .expect("expected postgrex dependency");
    let resolved = postgres.resolved_package.as_ref().unwrap();
    assert!(
        resolved
            .dependencies
            .iter()
            .any(|dep| dep.purl.as_deref() == Some("pkg:hex/db_connection"))
    );
    assert!(
        resolved
            .dependencies
            .iter()
            .any(|dep| dep.purl.as_deref() == Some("pkg:hex/decimal"))
    );

    let ecto = package_data
        .dependencies
        .iter()
        .find(|dep| dep.purl.as_deref() == Some("pkg:hex/ecto@3.12.4"))
        .expect("expected ecto dependency");
    let ecto_resolved = ecto.resolved_package.as_ref().unwrap();
    assert!(
        ecto_resolved
            .dependencies
            .iter()
            .any(|dep| dep.extracted_requirement.as_deref() == Some(">= 2.0.0"))
    );
}

#[test]
fn test_hex_mix_lock_ignores_non_hex_entries() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("mix.lock");
    std::fs::write(
        &path,
        "%{\n  \"local_dep\" => {:path, \"../local_dep\", \"abc123\"}\n}\n",
    )
    .unwrap();

    let package_data = HexLockParser::extract_first_package(&path);
    assert_eq!(package_data.package_type, Some(PackageType::Hex));
    assert_eq!(package_data.datasource_id, Some(DatasourceId::HexMixLock));
    assert!(package_data.dependencies.is_empty());
}

#[test]
fn test_hex_mix_lock_malformed_returns_default() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("mix.lock");
    std::fs::write(&path, "%{\n  \"plug\" => {:hex, :plug, \"1.0.0\"\n").unwrap();

    let package_data = HexLockParser::extract_first_package(&path);
    assert_eq!(package_data.package_type, Some(PackageType::Hex));
    assert_eq!(package_data.datasource_id, Some(DatasourceId::HexMixLock));
    assert!(package_data.dependencies.is_empty());
}
