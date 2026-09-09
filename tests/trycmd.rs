//! Documentation-style command snapshots for compact, stable output.

#[test]
fn command_snapshots_match_the_public_cli_contract() {
    let cases = trycmd::TestCases::new();
    cases.register_bin(
        "ketch",
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_ketch")),
    );
    // The version lives in `Cargo.toml` and nowhere else. Substituting it here
    // keeps the assertion exact — `--version` must report the crate version —
    // without a literal that every release pull request would have to edit,
    // and fail CI until someone did.
    cases
        .insert_var("[VERSION]", env!("CARGO_PKG_VERSION"))
        .expect("[VERSION] is a valid substitution name");
    cases.case("tests/cases/*.trycmd");
}
