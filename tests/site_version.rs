//! Keep the site version sourced from Cargo.toml, not a leftover 0.1.0.

const INDEX: &str = include_str!("../site/layouts/index.html");
const SEO: &str = include_str!("../site/layouts/partials/seo.html");
const SYNC: &str = include_str!("../site/sync-docs.py");

#[test]
fn homepage_chip_and_seo_use_site_params_version() {
    assert!(
        INDEX.contains("v{{ site.Params.version }} · preview"),
        "homepage chip must render site.Params.version marked preview"
    );
    assert!(
        !INDEX.contains("· caught"),
        "homepage chip must not use the old caught label"
    );
    assert!(
        !INDEX.contains("v0.1.0 · preview"),
        "homepage chip must not hard-code ketch 0.1.0"
    );
    assert!(
        SEO.contains("\"softwareVersion\" site.Params.version"),
        "SEO softwareVersion must render site.Params.version"
    );
    assert!(
        !SEO.contains("default \"0.1.0\""),
        "SEO must not fall back to a hard-coded 0.1.0"
    );
}

#[test]
fn sync_docs_writes_cargo_toml_into_site_params_version() {
    assert!(
        SYNC.contains("def sync_version"),
        "sync-docs.py must keep a version sync helper"
    );
    assert!(
        SYNC.contains("Cargo.toml"),
        "sync-docs.py must read the crate version from Cargo.toml"
    );
    assert!(
        SYNC.contains("site.Params.version") || SYNC.contains("version ="),
        "sync-docs.py must write site.Params.version"
    );
}
