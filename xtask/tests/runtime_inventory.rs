#[test]
fn every_firmware_task_case_has_an_explicit_gate_entry() {
    let file = syn::parse_file(include_str!("../../crates/device-runtime/tests/e2e.rs")).unwrap();
    let names = file
        .items
        .into_iter()
        .filter_map(|item| match item {
            syn::Item::Fn(function)
                if function
                    .attrs
                    .iter()
                    .any(|attribute| attribute.path().is_ident("test")) =>
            {
                assert!(
                    function.attrs.iter().all(|attribute| {
                        !attribute.path().is_ident("ignore")
                            && !attribute.path().is_ident("cfg_attr")
                    }),
                    "required firmware task tests must execute: {}",
                    function.sig.ident
                );
                Some(function.sig.ident.to_string())
            }
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    let expected = xtask::runtime_test_inventory::CASES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(!names.is_empty());
    assert_eq!(expected.len(), xtask::runtime_test_inventory::CASES.len());
    assert_eq!(names, expected);
    let report = expected
        .iter()
        .map(|name| format!("{name}: test\n"))
        .collect::<String>();
    assert!(
        xtask::evidence::verify_test_inventory(&report, xtask::runtime_test_inventory::CASES)
            .is_ok()
    );
    let missing = report.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert!(
        xtask::evidence::verify_test_inventory(&missing, xtask::runtime_test_inventory::CASES)
            .is_err()
    );
}
