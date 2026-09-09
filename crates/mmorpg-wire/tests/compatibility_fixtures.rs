use mmorpg_wire::{
    CompatibilityControl, DecodedFrame, MessageKind, SequencedServerMessage, ServerMessage,
    decode_compatibility_control, decode_one, encode_compatibility_control,
};

// Frozen v1 frame consumed by the previous-client decoder below. It is a
// sequenced Welcome from the development server and must remain decodable
// after future gameplay protocol versions are introduced.
const SEQUENCED_WELCOME_V1_FIXTURE_HEX: &str =
    "000000244d4d4f570001020000000018000000000000000101000d6d6d6f7270672d736572766572";

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

fn previous_client_decode_v1_frame(input: &[u8]) -> (DecodedFrame, SequencedServerMessage) {
    let decoded = decode_one(input).expect("previous client accepts v1 frame");
    assert_eq!(decoded.envelope.kind, MessageKind::Event);
    let message = SequencedServerMessage::decode_payload(&decoded.envelope.payload)
        .expect("previous client decodes sequenced server message");
    (decoded, message)
}

#[test]
fn previous_client_decodes_frozen_v1_gameplay_fixture() {
    let fixture = decode_hex(SEQUENCED_WELCOME_V1_FIXTURE_HEX);
    let (decoded, message) = previous_client_decode_v1_frame(&fixture);

    assert_eq!(decoded.consumed, fixture.len());
    assert_eq!(message.sequence, 1);
    assert_eq!(
        message.message,
        ServerMessage::Welcome {
            server: "mmorpg-server".to_owned(),
        }
    );
}

#[test]
fn previous_client_consumes_v1_frame_then_compatibility_control() {
    let gameplay = decode_hex(SEQUENCED_WELCOME_V1_FIXTURE_HEX);
    let rejection = decode_hex(VERSION_REJECTED_V1_FIXTURE_HEX);
    let mut stream = gameplay.clone();
    stream.extend_from_slice(&rejection);

    let (decoded, message) = previous_client_decode_v1_frame(&stream);
    assert_eq!(message.sequence, 1);

    let (control, consumed) = decode_compatibility_control(&stream[decoded.consumed..]).unwrap();
    assert_eq!(consumed, rejection.len());
    assert_eq!(
        control,
        CompatibilityControl::VersionRejected {
            supported_min: 1,
            supported_max: 1,
        }
    );
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
