use caqtdm_core::PutValue;
use caqtdm_datasource::DataSource;
use caqtdm_epics_pva::{
    decode_header, decode_search_payload, decode_size, discover_frame, encode_introspection,
    encode_size, search_frame, NtScalar, PvaClient, PvaServer, PvaSource, FLAG_BIG_ENDIAN, MAGIC,
    STRUCT_ID, VERSION,
};

#[test]
fn search_frame_matches_the_pvxs_layout() {
    let frame = search_frame(9, &[(3, "beam:current")], 5076, false);
    assert_eq!(frame[0], MAGIC);
    assert_eq!(frame[1], VERSION);
    assert_eq!(frame[2] & FLAG_BIG_ENDIAN, 0);
    let header = decode_header(&frame).unwrap();
    let payload = &frame[8..8 + header.payload_len as usize];
    let req = decode_search_payload(payload, false).unwrap();
    assert_eq!(req.sequence, 9);
    assert_eq!(req.flags, 0);
    assert_eq!(req.port, 5076);
    assert_eq!(req.protocols, vec!["tcp".to_string()]);
    assert_eq!(req.channels, vec![(3, "beam:current".to_string())]);
    assert_eq!(&payload[4 + 1 + 3..4 + 1 + 3 + 16], &[0u8; 16]);

    let discover = discover_frame(1, 5076);
    let header = decode_header(&discover).unwrap();
    let payload = &discover[8..8 + header.payload_len as usize];
    assert_eq!(payload[4], 0x01);

    assert_eq!(encode_size(3, false), vec![3]);
    assert_eq!(encode_size(300, false), vec![0xFE, 44, 1, 0, 0]);
    assert_eq!(decode_size(&[0xFF], false).unwrap().0, None);
}

#[test]
fn scalar_search_get_put_monitor() {
    let server = PvaServer::start(vec![("beam:current", 12.0)]).unwrap();
    let tcp = PvaClient::search(server.udp, "beam:current").unwrap();
    assert_eq!(tcp.port(), server.tcp.port());
    let mut client = PvaClient::connect(tcp).unwrap();
    client.create("beam:current").unwrap();
    let value = client.get("beam:current").unwrap();
    assert!((value - 12.0).abs() < 1e-9);
    let sub = client.subscribe("beam:current").unwrap();
    client.put("beam:current", 15.5).unwrap();
    let mut saw = false;
    for _ in 0..30 {
        for event in client.poll().unwrap() {
            if event.sub_id == sub && (event.value - 15.5).abs() < 1e-6 {
                saw = true;
            }
        }
        if saw {
            break;
        }
    }
    assert!(saw, "monitor followed the put");

    let mut source = PvaSource::connect(server.tcp).unwrap();
    let spec = caqtdm_core::parse_channel("pva://beam:current", "epics3");
    assert_eq!(spec.plugin, "epics4");
    let id = source.add_monitor(&spec).unwrap();
    source.set_value(&spec, &PutValue::Double(8.0)).unwrap();
    let mut found = false;
    for _ in 0..30 {
        for update in source.poll(0) {
            if update.id == id && (update.data.value.as_f64().unwrap() - 8.0).abs() < 1e-6 {
                found = true;
            }
        }
        if found {
            break;
        }
    }
    assert!(found);
}

#[test]
fn client_puts_and_gets_ntscalar_on_the_wire() {
    let server = PvaServer::start(vec![("beam:current", 1.0)]).unwrap();
    let mut client = PvaClient::connect(server.tcp).unwrap();
    client.create("beam:current").unwrap();
    let sample = NtScalar {
        value: 4.5,
        severity: 2,
        status: 3,
        message: "MAJOR".into(),
        seconds: 1_700_000_000,
        nanoseconds: 20,
        user_tag: 9,
    };
    let wire = encode_introspection();
    assert!(wire
        .windows(STRUCT_ID.len())
        .any(|w| w == STRUCT_ID.as_bytes()));
    client.put_nt("beam:current", &sample).unwrap();
    let got = client.get_nt("beam:current").unwrap();
    assert_eq!(got, sample);
    let scalar = client.get("beam:current").unwrap();
    assert!((scalar - 4.5).abs() < 1e-9);
}
