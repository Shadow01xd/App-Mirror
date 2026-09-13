use tyu_protocol::*;
use tyu_types::*;
#[test]
fn nearby_pair_request_appends_to_the_existing_schema() {
    let frame = Frame::new(Message::PairNearbyRequest { nonce: [7; 32] });
    let bytes = encode(&frame).unwrap();
    assert_eq!(&bytes[6..8], &33u16.to_be_bytes());
    assert_eq!(decode(&bytes).unwrap(), frame);
}
#[test]
fn roundtrip_and_frozen_envelope() {
    let f = Frame {
        request: RequestId(uuid::Uuid::nil()),
        session: None,
        message: Message::Ping(42),
    };
    let bytes = encode(&f).unwrap();
    assert_eq!(&bytes[..8], b"TYU1\x01\x00\x00\x0a");
    assert_eq!(decode(&bytes).unwrap(), f);
    // Golden fixture locks postcard enum discriminants, UUID and envelope framing.
    assert_eq!(
        bytes,
        [
            b"TYU1\x01\x00\x00\x0a\x00\x00\x00\x14".as_slice(),
            &[16],
            &[0; 16],
            &[0, 9, 42]
        ]
        .concat()
    );
}
#[test]
fn version_negotiation() {
    assert_eq!(
        negotiate(ProtocolVersion { major: 1, minor: 9 }).unwrap(),
        ProtocolVersion::V1
    );
    assert_eq!(
        negotiate(ProtocolVersion { major: 2, minor: 0 }),
        Err(TyuErrorCode::Version)
    );
}
#[test]
fn malformed_unknown_oversized_and_trailing() {
    for n in 0..12 {
        assert!(decode(&vec![0; n]).is_err());
    }
    let f = Frame::new(Message::Heartbeat);
    let valid = encode(&f).unwrap();
    let mut unknown = valid.clone();
    unknown[7] = 255;
    assert_eq!(decode(&unknown), Err(TyuErrorCode::UnknownType));
    let mut large = valid.clone();
    large[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(decode(&large), Err(TyuErrorCode::Limit));
    let mut trailing = valid.clone();
    trailing.push(0);
    assert!(decode(&trailing).is_err());
    let mut broken = valid.clone();
    broken[12] = 255;
    assert!(decode(&broken).is_err());
    let mut version = valid;
    version[4] = 2;
    assert_eq!(decode(&version), Err(TyuErrorCode::Version));
}
#[test]
fn limits_and_paths() {
    assert_eq!(
        encode(&Frame::new(Message::Clipboard(
            "a".repeat(MAX_CLIPBOARD + 1)
        ))),
        Err(TyuErrorCode::Limit)
    );
    for path in [
        "../x", "/root", "C:\\x", "a/b", "a\\b", "NUL.txt", "COM1", "a:", "a.", "..",
    ] {
        assert!(validate_filename(path).is_err(), "{path}");
    }
    assert!(validate_filename("foto válida.png").is_ok());
}
#[test]
fn input_roundtrip_and_mapping() {
    for event in [
        InputEvent::TouchDown {
            contact: 0,
            x: 0.2,
            y: 0.8,
        },
        InputEvent::TouchMove {
            contact: 1,
            x: 0.3,
            y: 0.2,
        },
        InputEvent::TouchUp {
            contact: 1,
            x: 1.0,
            y: 0.0,
        },
        InputEvent::MouseMove { x: 0.2, y: 0.4 },
        InputEvent::KeyDown { code: 25 },
        InputEvent::KeyUp { code: 25 },
        InputEvent::TextInput("á🙂".into()),
    ] {
        let mut f = Frame::new(Message::Input(event));
        f.session = Some(SessionId::new());
        assert_eq!(decode(&encode(&f).unwrap()).unwrap(), f);
    }
    assert!(
        !InputEvent::MouseMove {
            x: f32::NAN,
            y: 0.0
        }
        .valid()
    );
    assert!(
        !InputEvent::TouchDown {
            contact: 32,
            x: 0.0,
            y: 0.0
        }
        .valid()
    );
    assert_eq!(
        map_viewport(0.0, 1.0, Orientation::Landscape, 1920, 1080),
        Some((0.0, 0.0))
    );
    assert!(map_viewport(-0.1, 0.0, Orientation::Portrait, 1, 1).is_none());
}
#[test]
fn bounded_noise_never_panics() {
    let mut seed = 123456789u64;
    for len in 0..512 {
        let mut bytes = vec![0; len];
        for b in &mut bytes {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *b = seed as u8;
        }
        let _result = decode(&bytes);
    }
}
