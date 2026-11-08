use e1004_bsp::console::{parse, Command};
#[test]
fn image_commands_validate_identity_alignment_hex_and_exact_argument_counts() {
    for command in [
        "image begin 1 1 0 0",
        "image data 1 0 00112233",
        "image commit 1",
        "image abort 1",
        "image delete 8",
    ] {
        assert!(matches!(parse(command), Ok(Command::Media(_))), "{command}");
    }
    for command in [
        "image begin 1 0 0 0",
        "image begin 1 9 0 0",
        "image begin 1 1 4 0",
        "image data 1 1 00112233",
        "image data 1 0 4f112233",
        "image data 1 0 001122",
        "image data 1 0 zz112233",
        "image commit 1 extra",
        "image abort",
        "image delete 0",
    ] {
        assert!(parse(command).is_err(), "{command}");
    }
}
