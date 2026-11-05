#[cfg(kani)]
#[kani::proof]
fn rejects_a_false_contract() {
    let value: u8 = kani::any();
    assert!(value < 255);
}
