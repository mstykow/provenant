// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::scan_test_utils::{assert_dependency_present, scan_and_assemble};
    use crate::models::DatasourceId;

    #[test]
    fn test_gradle_scan_merges_build_and_lockfile_dependency_surfaces() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let gradle_dir = temp_dir.path().join("gradle");
        fs::create_dir_all(&gradle_dir).expect("create gradle dir");
        fs::copy(
            "testdata/gradle-golden/groovy/version-catalog/build.gradle",
            temp_dir.path().join("build.gradle"),
        )
        .expect("copy build.gradle fixture");
        fs::copy(
            "testdata/gradle-golden/groovy/version-catalog/gradle/libs.versions.toml",
            gradle_dir.join("libs.versions.toml"),
        )
        .expect("copy libs.versions.toml fixture");
        fs::copy(
            "testdata/gradle-lock/basic/gradle.lockfile",
            temp_dir.path().join("gradle.lockfile"),
        )
        .expect("copy gradle.lockfile fixture");

        let (files, result) = scan_and_assemble(temp_dir.path());

        assert!(result.packages.is_empty());
        assert_dependency_present(
            &result.dependencies,
            "pkg:maven/androidx.appcompat/appcompat@1.7.0",
            "build.gradle",
        );
        assert_dependency_present(
            &result.dependencies,
            "pkg:maven/org.springframework.boot/spring-boot-starter-web@2.7.0",
            "gradle.lockfile",
        );

        let build_gradle = files
            .iter()
            .find(|file| file.path.ends_with("/build.gradle"))
            .expect("build.gradle should be scanned");
        let gradle_lockfile = files
            .iter()
            .find(|file| file.path.ends_with("/gradle.lockfile"))
            .expect("gradle.lockfile should be scanned");

        assert!(build_gradle.for_packages.is_empty());
        assert!(gradle_lockfile.for_packages.is_empty());
        assert!(
            build_gradle
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::BuildGradle))
        );
        assert!(
            gradle_lockfile
                .package_data
                .iter()
                .any(|pkg_data| pkg_data.datasource_id == Some(DatasourceId::GradleLockfile))
        );
    }
}
