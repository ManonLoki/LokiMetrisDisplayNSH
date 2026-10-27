#[test]
fn rejects_empty_and_control_key_identifiers() {
    assert!(super::read("", "device").is_err());
    assert!(super::write("service", "a\n", "secret").is_err());
    assert!(super::delete("service", "").is_err());
}
