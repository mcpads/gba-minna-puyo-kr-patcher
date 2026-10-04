use super::*;

#[test]
fn source_identity_rejects_modified_and_truncated_inputs() {
    let mut bytes = vec![0u8; 256];
    bytes[0xa0..0xac].copy_from_slice(b"MINNADE PUYO");
    bytes[0xac..0xb0].copy_from_slice(b"APYJ");
    bytes[0xbd] = bytes[0xa0..0xbd]
        .iter()
        .fold(0u8, |a, &b| a.wrapping_sub(b))
        .wrapping_sub(0x19);
    let profile = Profile {
        id: "fixture".into(),
        size: bytes.len(),
        sha256: sha256(&bytes),
        title: "MINNADE PUYO".into(),
        game_code: "APYJ".into(),
        revision: 0,
    };
    profile.verify(&bytes).unwrap();
    bytes[0xf0] ^= 1;
    assert!(profile.verify(&bytes).is_err());
    assert!(profile.verify(&bytes[..20]).is_err());
}
