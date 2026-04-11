use std::sync::OnceLock;

pub const BUILD_VERSION: &str = match option_env!("PROVENANT_BUILD_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

const ATTRIBUTION_NOTICE: &str = "License detection uses data from ScanCode Toolkit (CC-BY-4.0). See NOTICE file or --show-attribution option.";

pub fn build_long_version() -> &'static str {
    static LONG_VERSION: OnceLock<String> = OnceLock::new();

    LONG_VERSION
        .get_or_init(|| format!("{BUILD_VERSION}\n{ATTRIBUTION_NOTICE}"))
        .as_str()
}
