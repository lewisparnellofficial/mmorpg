use mmorpg_wire::{
    CompatibilityControl, decode_compatibility_control, encode_compatibility_control,
};

// Frozen fixture consumed by a previous-client decoder. Keep this literal
// stable when gameplay protocol versions evolve.
const VERSION_REJECTED_V1_FIXTURE_HEX: &str = "000000114d4d4f5700000300000000050100010001";

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                _ => panic!("invalid fixture hex"),
            };
            digit(pair[0]) * 16 + digit(pair[1])
        })
        .collect()
}

#[test]
fn previous_client_decodes_frozen_version_rejection_fixture() {
    let fixture = decode_hex(VERSION_REJECTED_V1_FIXTURE_HEX);
    assert_eq!(
        decode_compatibility_control(&fixture),
        Ok((
            CompatibilityControl::VersionRejected {
                supported_min: 1,
                supported_max: 1,
            },
            fixture.len(),
        ))
    );
}

#[test]
fn compatibility_encoder_matches_frozen_previous_client_fixture() {
    let encoded = encode_compatibility_control(&CompatibilityControl::VersionRejected {
        supported_min: 1,
        supported_max: 1,
    });
    assert_eq!(encoded, decode_hex(VERSION_REJECTED_V1_FIXTURE_HEX));
}
