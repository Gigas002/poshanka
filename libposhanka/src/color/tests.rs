use super::{ParseHexRgbaError, parse_hex_rgba, parse_hex_rgba_to_bgra, rgba_to_bgra};

#[test]
fn parse_opaque_white() {
    assert_eq!(parse_hex_rgba("#ffffffff").unwrap(), [255, 255, 255, 255]);
}

#[test]
fn parse_semi_transparent() {
    assert_eq!(
        parse_hex_rgba("#11223380").unwrap(),
        [0x11, 0x22, 0x33, 0x80]
    );
}

#[test]
fn parse_rejects_short() {
    assert_eq!(
        parse_hex_rgba("#ff00").unwrap_err(),
        ParseHexRgbaError::InvalidFormat
    );
}

#[test]
fn bgra_reorder() {
    assert_eq!(rgba_to_bgra([1, 2, 3, 4]), [3, 2, 1, 4]);
}

#[test]
fn parse_to_bgra() {
    assert_eq!(
        parse_hex_rgba_to_bgra("#ff0000ff").unwrap(),
        [0, 0, 255, 255]
    );
}
