use gitpass_host::framing::*;
use serde_json::json;
use std::io::Cursor;

#[test]
fn roundtrip_multiple_messages() {
    let mut buf = vec![];
    write_message(&mut buf, &json!({"cmd":"status","id":1})).unwrap();
    write_message(&mut buf, &json!({"cmd":"lock","id":"x","s":"ünïcødé"})).unwrap();
    let body_len = serde_json::to_vec(&json!({"cmd":"status","id":1})).unwrap().len() as u32;
    assert_eq!(&buf[..4], &body_len.to_le_bytes());
    let mut r = Cursor::new(buf);
    assert_eq!(read_message(&mut r).unwrap().unwrap()["cmd"], "status");
    assert_eq!(read_message(&mut r).unwrap().unwrap()["s"], "ünïcødé");
    assert!(read_message(&mut r).unwrap().is_none(), "clean EOF");
}

#[test]
fn little_endian_header() {
    let mut r = Cursor::new(b"\x02\x00\x00\x00{}".to_vec());
    assert_eq!(read_message(&mut r).unwrap().unwrap(), json!({}));
}

#[test]
fn rejects_truncated_and_oversize_and_garbage() {
    assert!(read_message(&mut Cursor::new(vec![5u8, 0])).is_err());
    assert!(read_message(&mut Cursor::new(b"\x0a\x00\x00\x00{}".to_vec())).is_err());
    let big = ((MAX_IN + 1) as u32).to_le_bytes();
    assert!(read_message(&mut Cursor::new(big.to_vec())).is_err());
    assert!(read_message(&mut Cursor::new(b"\x03\x00\x00\x00abc".to_vec())).is_err());
    let huge = json!({"x": "a".repeat(MAX_OUT)});
    assert!(encode_message(&huge).is_err());
}
