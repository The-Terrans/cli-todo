use super::*;

#[test]
fn base64_pads_partial_chunks() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    assert_eq!(base64("界\n".as_bytes()), "55WMCg==");
}

#[test]
fn osc52_targets_the_system_clipboard() {
    assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
}
