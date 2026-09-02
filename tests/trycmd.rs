//! Documentation-style command snapshots for compact, stable output.

#[test]
fn command_snapshots_match_the_public_cli_contract() {
    let cases = trycmd::TestCases::new();
    cases.register_bin(
        "ketch",
        std::path::PathBuf::from(env!("CARGO_BIN_EXE_ketch")),
    );
    cases.case("tests/cases/*.trycmd");
}
