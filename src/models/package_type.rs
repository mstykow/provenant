//! Package type identifiers for package parsers.
//!
//! Each variant uniquely identifies the package ecosystem/registry type.
//! These are used in Package URL (purl) type fields and in the JSON output
//! as the `"type"` field of package data.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Package ecosystem/registry type identifier.
///
/// Identifies the package manager or ecosystem a package belongs to
/// (e.g., npm, PyPI, Maven, Cargo). Used as the `"type"` field in
/// ScanCode Toolkit-compatible JSON output.
///
/// This enum includes both standard purl types and ScanCode-specific types
/// for file format recognizers (e.g., `Jar`, `War`) and metadata sources
/// (e.g., `About`, `Readme`). For the official list of standardized purl types, see:
/// <https://github.com/package-url/purl-spec/blob/main/purl-types-index.json>
///
/// # Serialization
///
/// Variants serialize to lowercase/kebab-case strings matching the
/// Python reference values. The JSON output is identical to the
/// Python ScanCode Toolkit.
///
/// # Examples
///
/// ```ignore
/// use provenant::models::PackageType;
///
/// let pt = PackageType::Npm;
/// assert_eq!(pt.as_ref(), "npm");
/// assert_eq!(pt.to_string(), "npm");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageType {
    About,
    Alpm,
    Alpine,
    Android,
    AndroidLib,
    Autotools,
    Axis2,
    Bazel,
    Bitbake,
    Bower,
    Buck,
    Cab,
    Cargo,
    Carthage,
    Chef,
    Chrome,
    Cocoapods,
    Composer,
    Conan,
    Conda,
    Cpan,
    Cran,
    Dart,
    Deb,
    Deno,
    Docker,
    Dmg,
    Ear,
    Freebsd,
    Gem,
    Generic,
    Github,
    Golang,
    Hackage,
    Haxe,
    Helm,
    Hex,
    Installshield,
    Julia,
    Ios,
    Iso,
    Ivy,
    Jar,
    #[serde(rename = "jboss-service")]
    JbossService,
    #[serde(rename = "linux-distro")]
    LinuxDistro,
    Maven,
    Meson,
    Meteor,
    Nix,
    Mozilla,
    Npm,
    Nsis,
    Nuget,
    Opam,
    Osgi,
    #[serde(rename = "pnpm-lock")]
    PnpmLock,
    Pubspec,
    Pypi,
    Pixi,
    Publiccode,
    Readme,
    Rpm,
    Shar,
    Squashfs,
    Swift,
    Vcpkg,
    War,
    Winexe,
    #[serde(rename = "windows-update")]
    WindowsUpdate,
}

impl PackageType {
    /// Returns the string representation of this package type.
    ///
    /// This matches the serialized form used in JSON output.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::About => "about",
            Self::Alpm => "alpm",
            Self::Alpine => "alpine",
            Self::Android => "android",
            Self::AndroidLib => "android_lib",
            Self::Autotools => "autotools",
            Self::Axis2 => "axis2",
            Self::Bazel => "bazel",
            Self::Bitbake => "bitbake",
            Self::Bower => "bower",
            Self::Buck => "buck",
            Self::Cab => "cab",
            Self::Cargo => "cargo",
            Self::Carthage => "carthage",
            Self::Chef => "chef",
            Self::Chrome => "chrome",
            Self::Cocoapods => "cocoapods",
            Self::Composer => "composer",
            Self::Conan => "conan",
            Self::Conda => "conda",
            Self::Cpan => "cpan",
            Self::Cran => "cran",
            Self::Dart => "dart",
            Self::Deb => "deb",
            Self::Deno => "deno",
            Self::Docker => "docker",
            Self::Dmg => "dmg",
            Self::Ear => "ear",
            Self::Freebsd => "freebsd",
            Self::Gem => "gem",
            Self::Generic => "generic",
            Self::Github => "github",
            Self::Golang => "golang",
            Self::Hackage => "hackage",
            Self::Haxe => "haxe",
            Self::Helm => "helm",
            Self::Hex => "hex",
            Self::Installshield => "installshield",
            Self::Julia => "julia",
            Self::Ios => "ios",
            Self::Iso => "iso",
            Self::Ivy => "ivy",
            Self::Jar => "jar",
            Self::JbossService => "jboss-service",
            Self::LinuxDistro => "linux-distro",
            Self::Maven => "maven",
            Self::Meson => "meson",
            Self::Meteor => "meteor",
            Self::Nix => "nix",
            Self::Mozilla => "mozilla",
            Self::Npm => "npm",
            Self::Nsis => "nsis",
            Self::Nuget => "nuget",
            Self::Opam => "opam",
            Self::Osgi => "osgi",
            Self::PnpmLock => "pnpm-lock",
            Self::Pubspec => "pubspec",
            Self::Pypi => "pypi",
            Self::Pixi => "pixi",
            Self::Publiccode => "publiccode",
            Self::Readme => "readme",
            Self::Rpm => "rpm",
            Self::Shar => "shar",
            Self::Squashfs => "squashfs",
            Self::Swift => "swift",
            Self::Vcpkg => "vcpkg",
            Self::War => "war",
            Self::Winexe => "winexe",
            Self::WindowsUpdate => "windows-update",
        }
    }
}

impl AsRef<str> for PackageType {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for PackageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PackageType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let json = format!("\"{}\"", s);
        serde_json::from_str(&json).map_err(|_| format!("unknown package type: {}", s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization() {
        let pt = PackageType::Npm;
        let json = serde_json::to_string(&pt).unwrap();
        assert_eq!(json, r#""npm""#);
    }

    #[test]
    fn test_deserialization() {
        let json = r#""npm""#;
        let pt: PackageType = serde_json::from_str(json).unwrap();
        assert_eq!(pt, PackageType::Npm);
    }

    #[test]
    fn test_as_str() {
        assert_eq!(PackageType::Npm.as_str(), "npm");
        assert_eq!(PackageType::Cargo.as_str(), "cargo");
        assert_eq!(PackageType::Pypi.as_str(), "pypi");
        assert_eq!(PackageType::Alpm.as_str(), "alpm");
        assert_eq!(PackageType::Vcpkg.as_str(), "vcpkg");
        assert_eq!(PackageType::Hackage.as_str(), "hackage");
        assert_eq!(PackageType::Hex.as_str(), "hex");
    }

    #[test]
    fn test_display() {
        assert_eq!(PackageType::Npm.to_string(), "npm");
    }

    #[test]
    fn test_as_ref() {
        let pt = PackageType::Npm;
        let s: &str = pt.as_ref();
        assert_eq!(s, "npm");
    }

    #[test]
    fn test_kebab_case_variants() {
        assert_eq!(PackageType::JbossService.as_str(), "jboss-service");
        assert_eq!(PackageType::LinuxDistro.as_str(), "linux-distro");
        assert_eq!(PackageType::PnpmLock.as_str(), "pnpm-lock");
        assert_eq!(PackageType::Winexe.as_str(), "winexe");
        assert_eq!(PackageType::WindowsUpdate.as_str(), "windows-update");

        // Verify serialization matches
        let json = serde_json::to_string(&PackageType::JbossService).unwrap();
        assert_eq!(json, r#""jboss-service""#);

        let json = serde_json::to_string(&PackageType::LinuxDistro).unwrap();
        assert_eq!(json, r#""linux-distro""#);

        let json = serde_json::to_string(&PackageType::PnpmLock).unwrap();
        assert_eq!(json, r#""pnpm-lock""#);

        let json = serde_json::to_string(&PackageType::Winexe).unwrap();
        assert_eq!(json, r#""winexe""#);

        let json = serde_json::to_string(&PackageType::WindowsUpdate).unwrap();
        assert_eq!(json, r#""windows-update""#);
    }

    #[test]
    fn test_snake_case_variant() {
        assert_eq!(PackageType::AndroidLib.as_str(), "android_lib");

        let json = serde_json::to_string(&PackageType::AndroidLib).unwrap();
        assert_eq!(json, r#""android_lib""#);
    }

    #[test]
    fn test_deserialization_kebab_case() {
        let pt: PackageType = serde_json::from_str(r#""jboss-service""#).unwrap();
        assert_eq!(pt, PackageType::JbossService);

        let pt: PackageType = serde_json::from_str(r#""linux-distro""#).unwrap();
        assert_eq!(pt, PackageType::LinuxDistro);

        let pt: PackageType = serde_json::from_str(r#""pnpm-lock""#).unwrap();
        assert_eq!(pt, PackageType::PnpmLock);

        let pt: PackageType = serde_json::from_str(r#""winexe""#).unwrap();
        assert_eq!(pt, PackageType::Winexe);

        let pt: PackageType = serde_json::from_str(r#""windows-update""#).unwrap();
        assert_eq!(pt, PackageType::WindowsUpdate);
    }
}
