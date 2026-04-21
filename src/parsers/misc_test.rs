// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

//! Tests for file type recognizers in misc.rs

use crate::models::PackageType;

use super::PackageParser;
use super::misc::*;
use crate::models::DatasourceId;
use crate::parsers::try_parse_file;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// ============================================================================
// Java Archives
// ============================================================================

#[test]
fn test_java_jar_recognizer() {
    // Positive cases
    assert!(JavaJarRecognizer::is_match(&PathBuf::from(
        "lib/commons-lang3.jar"
    )));
    assert!(JavaJarRecognizer::is_match(&PathBuf::from(
        "/usr/share/java/test.jar"
    )));
    assert!(JavaJarRecognizer::is_match(&PathBuf::from("example.jar")));

    // Negative cases
    assert!(!JavaJarRecognizer::is_match(&PathBuf::from(
        "lib/example.war"
    )));
    assert!(!JavaJarRecognizer::is_match(&PathBuf::from(
        "lib/example.tar"
    )));
    assert!(!JavaJarRecognizer::is_match(&PathBuf::from("README.md")));

    // Extract packages
    let packages = JavaJarRecognizer::extract_packages(&PathBuf::from("test.jar"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Jar));
    assert_eq!(packages[0].datasource_id, Some(DatasourceId::JavaJar));
}

#[test]
fn test_ivy_xml_recognizer() {
    // Positive cases
    assert!(IvyXmlRecognizer::is_match(&PathBuf::from(
        "project/ivy.xml"
    )));
    assert!(IvyXmlRecognizer::is_match(&PathBuf::from(
        "/home/user/myapp/ivy.xml"
    )));

    // Negative cases
    assert!(!IvyXmlRecognizer::is_match(&PathBuf::from("ivy.xml.bak")));
    assert!(!IvyXmlRecognizer::is_match(&PathBuf::from("pom.xml")));
    assert!(!IvyXmlRecognizer::is_match(&PathBuf::from("ivyconfig.xml")));

    // Extract packages
    let packages = IvyXmlRecognizer::extract_packages(&PathBuf::from("ivy.xml"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Ivy));
    assert_eq!(packages[0].datasource_id, Some(DatasourceId::AntIvyXml));
}

#[test]
fn test_java_war_recognizer() {
    // Positive cases
    assert!(JavaWarRecognizer::is_match(&PathBuf::from(
        "webapps/myapp.war"
    )));
    assert!(JavaWarRecognizer::is_match(&PathBuf::from(
        "application.war"
    )));

    // Negative cases
    assert!(!JavaWarRecognizer::is_match(&PathBuf::from(
        "lib/example.jar"
    )));
    assert!(!JavaWarRecognizer::is_match(&PathBuf::from(
        "lib/example.ear"
    )));

    // Extract packages
    let packages = JavaWarRecognizer::extract_packages(&PathBuf::from("app.war"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::War));
    assert_eq!(
        packages[0].datasource_id,
        Some(DatasourceId::JavaWarArchive)
    );
}

#[test]
fn test_java_war_web_xml_recognizer() {
    // Positive cases
    assert!(JavaWarWebXmlRecognizer::is_match(&PathBuf::from(
        "WEB-INF/web.xml"
    )));
    assert!(JavaWarWebXmlRecognizer::is_match(&PathBuf::from(
        "myapp/WEB-INF/web.xml"
    )));

    // Negative cases
    assert!(!JavaWarWebXmlRecognizer::is_match(&PathBuf::from(
        "web.xml"
    )));
    assert!(!JavaWarWebXmlRecognizer::is_match(&PathBuf::from(
        "config/web.xml"
    )));
    assert!(!JavaWarWebXmlRecognizer::is_match(&PathBuf::from(
        "META-INF/web.xml"
    )));

    // Extract packages
    let packages = JavaWarWebXmlRecognizer::extract_packages(&PathBuf::from("WEB-INF/web.xml"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::War));
    assert_eq!(packages[0].datasource_id, Some(DatasourceId::JavaWarWebXml));
}

#[test]
fn test_java_ear_recognizer() {
    // Positive cases
    assert!(JavaEarRecognizer::is_match(&PathBuf::from(
        "deploy/app.ear"
    )));
    assert!(JavaEarRecognizer::is_match(&PathBuf::from(
        "enterprise.ear"
    )));

    // Negative cases
    assert!(!JavaEarRecognizer::is_match(&PathBuf::from(
        "lib/example.jar"
    )));
    assert!(!JavaEarRecognizer::is_match(&PathBuf::from(
        "lib/example.war"
    )));

    // Extract packages
    let packages = JavaEarRecognizer::extract_packages(&PathBuf::from("app.ear"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Ear));
    assert_eq!(
        packages[0].datasource_id,
        Some(DatasourceId::JavaEarArchive)
    );
}

#[test]
fn test_java_ear_app_xml_recognizer() {
    // Positive cases
    assert!(JavaEarAppXmlRecognizer::is_match(&PathBuf::from(
        "META-INF/application.xml"
    )));
    assert!(JavaEarAppXmlRecognizer::is_match(&PathBuf::from(
        "myapp/META-INF/application.xml"
    )));

    // Negative cases
    assert!(!JavaEarAppXmlRecognizer::is_match(&PathBuf::from(
        "application.xml"
    )));
    assert!(!JavaEarAppXmlRecognizer::is_match(&PathBuf::from(
        "config/application.xml"
    )));
    assert!(!JavaEarAppXmlRecognizer::is_match(&PathBuf::from(
        "WEB-INF/application.xml"
    )));

    // Extract packages
    let packages =
        JavaEarAppXmlRecognizer::extract_packages(&PathBuf::from("META-INF/application.xml"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Ear));
    assert_eq!(
        packages[0].datasource_id,
        Some(DatasourceId::JavaEarApplicationXml)
    );
}

// ============================================================================
// Apache Axis2
// ============================================================================

#[test]
fn test_axis2_module_xml_recognizer() {
    // Positive cases (case-insensitive)
    assert!(Axis2ModuleXmlRecognizer::is_match(&PathBuf::from(
        "meta-inf/module.xml"
    )));
    assert!(Axis2ModuleXmlRecognizer::is_match(&PathBuf::from(
        "META-INF/module.xml"
    )));
    assert!(Axis2ModuleXmlRecognizer::is_match(&PathBuf::from(
        "mymodule/META-INF/module.xml"
    )));

    // Negative cases
    assert!(!Axis2ModuleXmlRecognizer::is_match(&PathBuf::from(
        "module.xml"
    )));
    assert!(!Axis2ModuleXmlRecognizer::is_match(&PathBuf::from(
        "config/module.xml"
    )));

    // Extract packages
    let packages =
        Axis2ModuleXmlRecognizer::extract_packages(&PathBuf::from("META-INF/module.xml"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Axis2));
    assert_eq!(
        packages[0].datasource_id,
        Some(DatasourceId::Axis2ModuleXml)
    );
}

#[test]
fn test_axis2_mar_recognizer() {
    // Positive cases
    assert!(Axis2MarRecognizer::is_match(&PathBuf::from(
        "modules/mymodule.mar"
    )));
    assert!(Axis2MarRecognizer::is_match(&PathBuf::from("example.mar")));

    // Negative cases
    assert!(!Axis2MarRecognizer::is_match(&PathBuf::from("example.jar")));
    assert!(!Axis2MarRecognizer::is_match(&PathBuf::from("example.tar")));

    // Extract packages
    let packages = Axis2MarRecognizer::extract_packages(&PathBuf::from("module.mar"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Axis2));
    assert_eq!(packages[0].datasource_id, Some(DatasourceId::Axis2Mar));
}

// ============================================================================
// JBoss
// ============================================================================

#[test]
fn test_jboss_sar_recognizer() {
    // Positive cases
    assert!(JBossSarRecognizer::is_match(&PathBuf::from(
        "deploy/myservice.sar"
    )));
    assert!(JBossSarRecognizer::is_match(&PathBuf::from("service.sar")));

    // Negative cases
    assert!(!JBossSarRecognizer::is_match(&PathBuf::from("service.jar")));
    assert!(!JBossSarRecognizer::is_match(&PathBuf::from("service.war")));

    // Extract packages
    let packages = JBossSarRecognizer::extract_packages(&PathBuf::from("service.sar"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::JbossService));
    assert_eq!(packages[0].datasource_id, Some(DatasourceId::JbossSar));
}

#[test]
fn test_jboss_service_xml_recognizer() {
    // Positive cases (case-insensitive)
    assert!(JBossServiceXmlRecognizer::is_match(&PathBuf::from(
        "meta-inf/jboss-service.xml"
    )));
    assert!(JBossServiceXmlRecognizer::is_match(&PathBuf::from(
        "META-INF/jboss-service.xml"
    )));
    assert!(JBossServiceXmlRecognizer::is_match(&PathBuf::from(
        "myservice/META-INF/jboss-service.xml"
    )));

    // Negative cases
    assert!(!JBossServiceXmlRecognizer::is_match(&PathBuf::from(
        "jboss-service.xml"
    )));
    assert!(!JBossServiceXmlRecognizer::is_match(&PathBuf::from(
        "config/jboss-service.xml"
    )));

    // Extract packages
    let packages =
        JBossServiceXmlRecognizer::extract_packages(&PathBuf::from("META-INF/jboss-service.xml"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::JbossService));
    assert_eq!(
        packages[0].datasource_id,
        Some(DatasourceId::JbossServiceXml)
    );
}

// ============================================================================
// Meteor
// ============================================================================

#[test]
fn test_meteor_package_recognizer() {
    // Positive cases
    assert!(MeteorPackageRecognizer::is_match(&PathBuf::from(
        "mypackage/package.js"
    )));
    assert!(MeteorPackageRecognizer::is_match(&PathBuf::from(
        "packages/local/package.js"
    )));

    // Negative cases
    assert!(!MeteorPackageRecognizer::is_match(&PathBuf::from(
        "package.js"
    )));
    assert!(!MeteorPackageRecognizer::is_match(&PathBuf::from(
        "package.json"
    )));
    assert!(!MeteorPackageRecognizer::is_match(&PathBuf::from(
        "src/package.js.bak"
    )));

    // Extract packages
    let packages =
        MeteorPackageRecognizer::extract_packages(&PathBuf::from("mypackage/package.js"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Meteor));
    assert_eq!(packages[0].datasource_id, Some(DatasourceId::MeteorPackage));
}

// ============================================================================
// Mobile Apps
// ============================================================================

#[test]
fn test_android_library_recognizer() {
    // Positive cases
    assert!(AndroidLibraryRecognizer::is_match(&PathBuf::from(
        "libs/mylib.aar"
    )));
    assert!(AndroidLibraryRecognizer::is_match(&PathBuf::from(
        "library.aar"
    )));

    // Negative cases
    assert!(!AndroidLibraryRecognizer::is_match(&PathBuf::from(
        "app.apk"
    )));
    assert!(!AndroidLibraryRecognizer::is_match(&PathBuf::from(
        "library.jar"
    )));

    // Extract packages
    let packages = AndroidLibraryRecognizer::extract_packages(&PathBuf::from("library.aar"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::AndroidLib));
    assert_eq!(
        packages[0].datasource_id,
        Some(DatasourceId::AndroidAarLibrary)
    );
}

#[test]
fn test_mozilla_xpi_recognizer() {
    // Positive cases
    assert!(MozillaXpiRecognizer::is_match(&PathBuf::from(
        "extensions/myext.xpi"
    )));
    assert!(MozillaXpiRecognizer::is_match(&PathBuf::from("addon.xpi")));

    // Negative cases
    assert!(!MozillaXpiRecognizer::is_match(&PathBuf::from(
        "extension.zip"
    )));
    assert!(!MozillaXpiRecognizer::is_match(&PathBuf::from("addon.crx")));

    // Extract packages
    let packages = MozillaXpiRecognizer::extract_packages(&PathBuf::from("addon.xpi"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Mozilla));
    assert_eq!(packages[0].datasource_id, Some(DatasourceId::MozillaXpi));
}

#[test]
fn test_chrome_crx_recognizer() {
    // Positive cases
    assert!(ChromeCrxRecognizer::is_match(&PathBuf::from(
        "extensions/myext.crx"
    )));
    assert!(ChromeCrxRecognizer::is_match(&PathBuf::from(
        "extension.crx"
    )));

    // Negative cases
    assert!(!ChromeCrxRecognizer::is_match(&PathBuf::from(
        "extension.zip"
    )));
    assert!(!ChromeCrxRecognizer::is_match(&PathBuf::from("addon.xpi")));

    // Extract packages
    let packages = ChromeCrxRecognizer::extract_packages(&PathBuf::from("extension.crx"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Chrome));
    assert_eq!(packages[0].datasource_id, Some(DatasourceId::ChromeCrx));
}

#[test]
fn test_ios_ipa_recognizer() {
    // Positive cases
    assert!(IosIpaRecognizer::is_match(&PathBuf::from("apps/MyApp.ipa")));
    assert!(IosIpaRecognizer::is_match(&PathBuf::from(
        "application.ipa"
    )));

    // Negative cases
    assert!(!IosIpaRecognizer::is_match(&PathBuf::from("app.apk")));
    assert!(!IosIpaRecognizer::is_match(&PathBuf::from("app.zip")));

    // Extract packages
    let packages = IosIpaRecognizer::extract_packages(&PathBuf::from("MyApp.ipa"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Ios));
    assert_eq!(packages[0].datasource_id, Some(DatasourceId::IosIpa));
}

// ============================================================================
// Archives
// ============================================================================

#[test]
fn test_cab_archive_recognizer() {
    // Positive cases
    assert!(CabArchiveRecognizer::is_match(&PathBuf::from(
        "installer/setup.cab"
    )));
    assert!(CabArchiveRecognizer::is_match(&PathBuf::from(
        "archive.cab"
    )));

    // Negative cases
    assert!(!CabArchiveRecognizer::is_match(&PathBuf::from(
        "archive.zip"
    )));
    assert!(!CabArchiveRecognizer::is_match(&PathBuf::from("setup.exe")));

    // Extract packages
    let packages = CabArchiveRecognizer::extract_packages(&PathBuf::from("archive.cab"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Cab));
    assert_eq!(
        packages[0].datasource_id,
        Some(DatasourceId::MicrosoftCabinet)
    );
}

#[test]
fn test_shar_archive_recognizer() {
    // Positive cases
    assert!(SharArchiveRecognizer::is_match(&PathBuf::from(
        "archive/data.shar"
    )));
    assert!(SharArchiveRecognizer::is_match(&PathBuf::from(
        "package.shar"
    )));

    // Negative cases
    assert!(!SharArchiveRecognizer::is_match(&PathBuf::from(
        "package.tar"
    )));
    assert!(!SharArchiveRecognizer::is_match(&PathBuf::from(
        "script.sh"
    )));

    // Extract packages
    let packages = SharArchiveRecognizer::extract_packages(&PathBuf::from("archive.shar"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Shar));
    assert_eq!(
        packages[0].datasource_id,
        Some(DatasourceId::SharShellArchive)
    );
}

// ============================================================================
// Disk Images
// ============================================================================

#[test]
fn test_apple_dmg_recognizer() {
    // Positive cases
    assert!(AppleDmgRecognizer::is_match(&PathBuf::from(
        "installers/App.dmg"
    )));
    assert!(AppleDmgRecognizer::is_match(&PathBuf::from(
        "disk.sparseimage"
    )));
    assert!(AppleDmgRecognizer::is_match(&PathBuf::from("MyApp.dmg")));

    // Negative cases
    assert!(!AppleDmgRecognizer::is_match(&PathBuf::from("disk.iso")));
    assert!(!AppleDmgRecognizer::is_match(&PathBuf::from("archive.zip")));

    // Extract packages
    let packages = AppleDmgRecognizer::extract_packages(&PathBuf::from("App.dmg"));
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Dmg));
    assert_eq!(packages[0].datasource_id, Some(DatasourceId::AppleDmg));

    let packages_sparse = AppleDmgRecognizer::extract_packages(&PathBuf::from("disk.sparseimage"));
    assert_eq!(packages_sparse[0].package_type, Some(PackageType::Dmg));
}

#[test]
fn test_iso_image_recognizer() {
    // Positive cases
    assert!(IsoImageRecognizer::is_match(&PathBuf::from(
        "images/ubuntu.iso"
    )));
    assert!(IsoImageRecognizer::is_match(&PathBuf::from("disk.udf")));
    assert!(IsoImageRecognizer::is_match(&PathBuf::from("recovery.img")));

    // Negative cases
    assert!(!IsoImageRecognizer::is_match(&PathBuf::from("disk.dmg")));
    assert!(!IsoImageRecognizer::is_match(&PathBuf::from("archive.zip")));

    // Extract packages
    let packages_iso = IsoImageRecognizer::extract_packages(&PathBuf::from("ubuntu.iso"));
    assert_eq!(packages_iso.len(), 1);
    assert_eq!(packages_iso[0].package_type, Some(PackageType::Iso));
    assert_eq!(
        packages_iso[0].datasource_id,
        Some(DatasourceId::IsoDiskImage)
    );

    let packages_udf = IsoImageRecognizer::extract_packages(&PathBuf::from("disk.udf"));
    assert_eq!(packages_udf[0].package_type, Some(PackageType::Iso));

    let packages_img = IsoImageRecognizer::extract_packages(&PathBuf::from("recovery.img"));
    assert_eq!(packages_img[0].package_type, Some(PackageType::Iso));
}

// ============================================================================
// Magic byte recognizers
// ============================================================================

#[test]
fn test_try_parse_file_ignores_malformed_non_alpine_apk_fixture() {
    let temp_dir = TempDir::new().expect("temp dir");
    let apk_path = temp_dir.path().join("broken.apk");
    fs::write(&apk_path, b"not an alpine or android apk").expect("write broken apk");

    assert!(try_parse_file(&apk_path).is_none());
}

#[test]
fn test_try_parse_file_ignores_placeholder_alpine_apk_fixture() {
    assert!(try_parse_file(&PathBuf::from("testdata/misc/test_alpine.apk")).is_none());
}

#[test]
fn test_squashfs_recognizer() {
    let squashfs = PathBuf::from("testdata/misc/test.squashfs");

    assert!(SquashfsRecognizer::is_match(&squashfs));
    assert!(!SquashfsRecognizer::is_match(&PathBuf::from(
        "not_squashfs.bin"
    )));

    let packages = SquashfsRecognizer::extract_packages(&squashfs);
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Squashfs));
    assert_eq!(
        packages[0].datasource_id,
        Some(DatasourceId::SquashfsDiskImage)
    );
}

#[test]
fn test_nsis_recognizer() {
    let nsis = PathBuf::from("testdata/misc/test_nsis.exe");
    let zip_exe = PathBuf::from("testdata/misc/test_zip.exe");

    assert!(NsisRecognizer::is_match(&nsis));
    assert!(!NsisRecognizer::is_match(&zip_exe));
    assert!(!NsisRecognizer::is_match(&PathBuf::from("app.exe")));

    let packages = NsisRecognizer::extract_packages(&nsis);
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Nsis));
    assert_eq!(packages[0].datasource_id, Some(DatasourceId::NsisInstaller));
}

#[test]
fn test_installshield_recognizer() {
    let zip_exe = PathBuf::from("testdata/misc/test_zip.exe");
    let nsis = PathBuf::from("testdata/misc/test_nsis.exe");

    assert!(InstallShieldRecognizer::is_match(&zip_exe));
    assert!(!InstallShieldRecognizer::is_match(&nsis));
    assert!(!InstallShieldRecognizer::is_match(&PathBuf::from(
        "app.msi"
    )));

    let packages = InstallShieldRecognizer::extract_packages(&zip_exe);
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_type, Some(PackageType::Installshield));
    assert_eq!(
        packages[0].datasource_id,
        Some(DatasourceId::InstallshieldInstaller)
    );
}

// ============================================================================
// Verify all recognizers return minimal PackageData
// ============================================================================

#[test]
fn test_minimal_package_data_structure() {
    let packages = JavaJarRecognizer::extract_packages(&PathBuf::from("test.jar"));
    let pkg = &packages[0];

    // Should have package_type and datasource_id
    assert!(pkg.package_type.is_some());
    assert!(pkg.datasource_id.is_some());

    // All other fields should be None/empty (default)
    assert!(pkg.name.is_none());
    assert!(pkg.version.is_none());
    assert!(pkg.description.is_none());
    assert!(pkg.homepage_url.is_none());
    assert!(pkg.purl.is_none());
    assert_eq!(pkg.dependencies.len(), 0);
    assert_eq!(pkg.parties.len(), 0);
}
