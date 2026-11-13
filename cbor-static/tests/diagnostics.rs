#![forbid(unsafe_code)]

#[test]
#[ignore = "the expected stderr is one rustc's: `camas diagnostics` runs it on the pinned stable toolchain"]
fn derive_diagnostics() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass/*.rs");
    cases.compile_fail("tests/ui/*.rs");
}
