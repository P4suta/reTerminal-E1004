use super::TextBuffer;
use core::fmt::Write;

#[kani::proof]
#[kani::unwind(17)]
fn text_buffer_keeps_utf8_and_reports_overflow() {
    let input: [u8; 8] = kani::any();
    let length: usize = kani::any();
    kani::assume(length <= input.len());
    let mut output = TextBuffer::<4>::default();
    let result = output.append_bytes(&input[..length]);
    assert_eq!(result.is_ok(), length <= 4);
    assert!(output.len <= 4);
    if length <= 4 {
        assert_eq!(output.len, length);
        for index in 0..4 {
            if index < length {
                assert_eq!(output.bytes[index], input[index]);
            } else {
                assert_eq!(output.bytes[index], 0);
            }
        }
        kani::cover!(length == 4);
    } else {
        assert_eq!(output.len, 0);
        for byte in output.bytes {
            assert_eq!(byte, 0);
        }
        kani::cover!(length == 8);
    }
    let prefix = output.bytes;
    let prefix_length = output.len;
    let next = output.write_str("x");
    assert_eq!(next.is_err(), length == 4);
    assert!(output.len <= 4);
    for index in 0..4 {
        if index < prefix_length {
            assert_eq!(output.bytes[index], prefix[index]);
        }
    }
    if next.is_ok() {
        assert_eq!(output.len, prefix_length + 1);
        assert_eq!(output.bytes[prefix_length], b'x');
    } else {
        assert_eq!(output.len, prefix_length);
    }
    assert_eq!(output.failed, length >= 4);
    let error_view = TextBuffer::<4> {
        bytes: kani::any(),
        len: kani::any(),
        failed: true,
    };
    kani::assume(error_view.len <= 4);
    let expected = b"TEXT OVERFLOW";
    let actual = error_view.as_str().as_bytes();
    assert_eq!(actual.len(), expected.len());
    for index in 0..13 {
        assert_eq!(actual[index], expected[index]);
    }
}
