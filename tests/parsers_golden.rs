// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "golden-tests")]

pub use provenant::models;

mod parsers {
    pub use provenant::parsers::*;

    pub mod golden_test_utils {
        pub use provenant::parsers::golden_test_utils::*;
    }

    macro_rules! reexport_parser_items {
        ($($module:ident),+ $(,)?) => {
            $(
                pub mod $module {
                    #[allow(unused_imports)]
                    pub use provenant::parsers::*;
                }
            )+
        };
    }

    reexport_parser_items!(
        about,
        alpine,
        android,
        arch,
        autotools,
        bazel,
        bitbake,
        bower,
        buck,
        bun_lockb,
        cargo,
        cargo_lock,
        carthage,
        chef,
        clojure,
        composer,
        conan,
        conan_data,
        conda,
        conda_meta_json,
        cpan,
        cpan_dist_ini,
        cpan_makefile_pl,
        cran,
        dart,
        debian,
        docker,
        erlang_otp,
        freebsd,
        gitmodules,
        go,
        go_mod_graph,
        gradle,
        gradle_lock,
        helm,
        hex_lock,
        julia,
        maven,
        meson,
        microsoft_update_manifest,
        npm,
        npm_lock,
        npm_workspace,
        nuget,
        opam,
        os_release,
        pip_inspect_deplock,
        pixi,
        pnpm_lock,
        podfile,
        podfile_lock,
        podspec,
        podspec_json,
        readme,
        rpm_db,
        rpm_license_files,
        rpm_mariner_manifest,
        rpm_parser,
        rpm_specfile,
        rpm_yumdb,
        ruby,
        sbt,
        swift_manifest_json,
        swift_resolved,
        swift_show_dependencies,
        vcpkg,
        yarn_lock,
    );

    pub mod compiled_binary {
        #[allow(unused_imports)]
        pub use provenant::parsers::try_parse_compiled_bytes;
        #[allow(unused_imports)]
        pub use provenant::parsers::*;
    }

    pub mod windows_executable {
        #[allow(unused_imports)]
        pub use provenant::parsers::try_parse_windows_executable_bytes;
        #[allow(unused_imports)]
        pub use provenant::parsers::*;
    }
}

#[path = "../src/parsers/golden_test.rs"]
mod golden_test;
