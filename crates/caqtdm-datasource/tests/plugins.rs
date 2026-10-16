use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use caqtdm_core::PutValue;
use caqtdm_datasource::{
    build_archive_url, decode_bsread, decode_response, encode_bsread, encode_hello, encode_node_id,
    encode_read_request, encode_request, exchange, fetch_archiver, fetch_bsread, greeting,
    parse_fixture, parse_greeting, read_node, recv_bsread, samples_to_json, seal_secure_chunk,
    send_bsread, serve_archiver, serve_bsread, serve_loopback, serve_read, serve_zmtp, BsMessage,
    BsreadSource, DataSource, EnvSource, GpsSource, Hub, InternSource, Mbap, ModbusSource, NodeId,
    OpcuaSource, Pdu, RegisterMap, SimSource, POLICY_BASIC256SHA256, READ_REQUEST, VARIANT_DOUBLE,
};

#[test]
fn hub_routes_intern_writes() {
    let mut hub = Hub::new("epics3");
    hub.register(Box::new(InternSource::new()));
    hub.register(Box::new(SimSource::new("sim")));
    let id = hub.monitor("intern://SETPOINT").unwrap();
    hub.put("intern://SETPOINT", &PutValue::Double(12.5))
        .unwrap();
    let updates = hub.poll(0);
    let hit = updates.iter().find(|(i, _)| *i == id).unwrap();
    assert!((hit.1.value.as_f64().unwrap() - 12.5).abs() < 1e-9);

    let sim = hub.monitor("sim://const:4").unwrap();
    let updates = hub.poll(0);
    let hit = updates.iter().find(|(i, _)| *i == sim).unwrap();
    assert!((hit.1.value.as_f64().unwrap() - 4.0).abs() < 1e-9);
}

#[test]
fn demo_patterns_cover_the_catalog() {
    let mut sim = SimSource::new("demo");
    for name in [
        "const:3.5",
        "ramp:0:10:1000",
        "square:1000",
        "enum:Off,On",
        "wave:4:1000",
        "string:hello",
        "const:95?alarm=1",
    ] {
        let spec = caqtdm_core::parse_channel(name, "demo");
        sim.add_monitor(&spec).unwrap();
    }
    let updates = sim.poll(100);
    assert_eq!(updates.len(), 7);
    assert!((updates[0].data.value.as_f64().unwrap() - 3.5).abs() < 1e-9);
    assert!(updates[1].data.value.as_f64().unwrap() > 0.0);
    assert!(updates[2].data.value.as_f64().is_some());
    assert!(!updates[3].data.enum_strings.is_empty());
    assert_eq!(updates[4].data.value.as_f64_slice().len(), 4);
    assert_eq!(updates[5].data.value.as_text(), "hello");
    assert_eq!(updates[6].data.severity, caqtdm_core::Severity::Major);
}

#[test]
fn sine_is_deterministic() {
    let mut sim = SimSource::new("sim");
    let spec = caqtdm_core::parse_channel("sine:10:1000:0", "sim");
    let id = sim.add_monitor(&spec).unwrap();
    let a = sim.poll(250);
    let b = sim.poll(250);
    assert_eq!(a[0].id, id);
    assert_eq!(a[0].data.value, b[0].data.value);
    let c = sim.poll(500);
    assert_ne!(a[0].data.value, c[0].data.value);
}

#[test]
fn env_overlay_and_gps_clock() {
    let mut env = EnvSource::new();
    env.set_overlay("CAQTDM_TEST", "beamline");
    let spec = caqtdm_core::parse_channel("CAQTDM_TEST", "env");
    let id = env.add_monitor(&spec).unwrap();
    let updates = env.poll(0);
    assert_eq!(updates[0].id, id);
    assert_eq!(updates[0].data.value.as_text(), "beamline");
    assert!(env.set_value(&spec, &PutValue::String("x".into())).is_err());

    let mut gps = GpsSource::new();
    let spec = caqtdm_core::parse_channel("utc", "gps");
    gps.add_monitor(&spec).unwrap();
    let updates = gps.poll(1_700_000_000_000);
    assert!(updates[0].data.value.as_text().ends_with('Z'));
}

#[test]
fn archive_fixture_filters_time() {
    let text = "\
channel SR:VAC:P
0,1.0,0
1000,2.0,1
2000,9.0,2
";
    let series = parse_fixture(text).unwrap();
    assert_eq!(series["SR:VAC:P"].len(), 3);
    let url = build_archive_url("http://archive.local", "SR:VAC:P", 0, 1000);
    assert!(url.contains("from=0"));
    let mut src = caqtdm_datasource::ArchiveSource::from_fixture(text).unwrap();
    let spec = caqtdm_core::parse_channel("SR:VAC:P?from=0&to=1000", "archive");
    src.add_monitor(&spec).unwrap();
    let updates = src.poll(0);
    assert_eq!(updates[0].data.value.as_f64_slice(), vec![1.0, 2.0]);
}

#[test]
fn modbus_codec_and_loopback() {
    let mbap = Mbap {
        transaction: 7,
        protocol: 0,
        unit: 1,
    };
    let frame = encode_request(
        &mbap,
        &Pdu::ReadHolding {
            address: 0,
            count: 2,
        },
    )
    .unwrap();
    let (hdr, pdu) = decode_response(&frame).unwrap();
    assert_eq!(hdr.transaction, 7);
    assert_eq!(
        pdu,
        Pdu::ReadHolding {
            address: 0,
            count: 2
        }
    );

    let mut map = RegisterMap::new();
    map.set(0, 42);
    map.set(1, 7);
    let stop = Arc::new(AtomicBool::new(false));
    let addr = serve_loopback(map, Arc::clone(&stop)).unwrap();
    let reply = exchange(
        addr,
        &mbap,
        &Pdu::ReadHolding {
            address: 0,
            count: 2,
        },
    )
    .unwrap();
    assert_eq!(
        reply,
        Pdu::ReadHoldingResponse {
            registers: vec![42, 7]
        }
    );
    stop.store(true, Ordering::Relaxed);

    let mut local = ModbusSource::new(RegisterMap::new());
    let spec = caqtdm_core::parse_channel("40001", "modbus");
    local.set_value(&spec, &PutValue::Int(9)).unwrap();
    local.add_monitor(&spec).unwrap();
    let updates = local.poll(0);
    assert_eq!(updates[0].data.value.as_i64(), Some(9));
}

#[test]
fn opcua_and_bsread_partials() {
    let mut opc = OpcuaSource::new();
    opc.insert_double("ns=2;s=Temperature", 21.5);
    let spec = caqtdm_core::parse_channel("ns=2;s=Temperature", "opcua");
    opc.add_monitor(&spec).unwrap();
    opc.set_value(&spec, &PutValue::Double(22.0)).unwrap();
    let updates = opc.poll(0);
    assert!((updates[0].data.value.as_f64().unwrap() - 22.0).abs() < 1e-9);

    let mut map = RegisterMap::new();
    map.set_coil(0, true);
    map.set_coil(2, true);
    map.set_discrete(1, true);
    map.set_input(3, 99);
    let coils = map.handle(&Pdu::ReadBits {
        function: caqtdm_datasource::FN_READ_COILS,
        address: 0,
        count: 3,
    });
    assert_eq!(
        coils,
        Pdu::ReadBitsResponse {
            function: caqtdm_datasource::FN_READ_COILS,
            bits: vec![true, false, true],
        }
    );
    let discrete = map.handle(&Pdu::ReadBits {
        function: caqtdm_datasource::FN_READ_DISCRETE,
        address: 0,
        count: 2,
    });
    match discrete {
        Pdu::ReadBitsResponse { bits, .. } => assert_eq!(bits, vec![false, true]),
        other => panic!("{other:?}"),
    }
    let inputs = map.handle(&Pdu::ReadInput {
        address: 3,
        count: 1,
    });
    assert_eq!(
        inputs,
        Pdu::ReadInputResponse {
            registers: vec![99]
        }
    );
    let written = map.handle(&Pdu::WriteCoil {
        address: 4,
        on: true,
    });
    assert_eq!(
        written,
        Pdu::WriteCoil {
            address: 4,
            on: true
        }
    );
    let many = map.handle(&Pdu::WriteCoils {
        address: 5,
        bits: vec![true, false, true],
    });
    assert!(matches!(
        many,
        Pdu::WriteCoilsResponse {
            address: 5,
            count: 3
        }
    ));
    let frame = encode_request(
        &Mbap {
            transaction: 1,
            protocol: 0,
            unit: 1,
        },
        &Pdu::WriteCoil {
            address: 4,
            on: true,
        },
    )
    .unwrap();
    let (_, pdu) = decode_response(&frame).unwrap();
    assert_eq!(
        pdu,
        Pdu::WriteCoil {
            address: 4,
            on: true
        }
    );

    let mut bs = BsreadSource::new();
    let spec = caqtdm_core::parse_channel("pulse_id", "bsread");
    bs.add_monitor(&spec).unwrap();
    let a = bs.poll(0);
    let b = bs.poll(50);
    assert!(b[0].data.value.as_i64().unwrap() > a[0].data.value.as_i64().unwrap());
}

#[test]
fn archive_json_opcua_read_and_bsread_frame() {
    let samples = parse_fixture("channel SR:VAC:P\n1000,1.5,1\n").unwrap();
    let json = samples_to_json("SR:VAC:P", &samples["SR:VAC:P"]);
    let stop = Arc::new(AtomicBool::new(false));
    let addr = serve_archiver(json, Arc::clone(&stop)).unwrap();
    let fetched = fetch_archiver(addr, "SR:VAC:P").unwrap();
    assert_eq!(fetched[0].time_ms, 1000);
    assert!((fetched[0].value - 1.5).abs() < 1e-9);
    stop.store(true, Ordering::Relaxed);

    let node = NodeId::Numeric { ns: 2, id: 15 };
    let encoded = encode_node_id(&node);
    assert_eq!(encoded, vec![0x00, 2, 15]);
    let wide = encode_node_id(&NodeId::Numeric { ns: 2, id: 631 });
    assert_eq!(wide[0], 0x01);
    let hello = encode_hello("opc.tcp://127.0.0.1");
    assert_eq!(&hello[0..3], b"HEL");
    let request = encode_read_request(&node, 7);
    assert!(request.windows(2).any(|w| w == 631u16.to_le_bytes()));
    assert_eq!(READ_REQUEST, 631);
    assert_eq!(VARIANT_DOUBLE, 11);
    let stop = Arc::new(AtomicBool::new(false));
    let addr = serve_read(21.5, Arc::clone(&stop)).unwrap();
    let value = read_node(addr, &node).unwrap();
    assert!((value - 21.5).abs() < 1e-9);
    stop.store(true, Ordering::Relaxed);

    let message = BsMessage {
        pulse_id: 42,
        channel: "SARFE10-PBPG050:PHOTON-ENERGY-PER-PULSE".into(),
        value: 3.25,
    };
    assert_eq!(decode_bsread(&encode_bsread(&message)).unwrap(), message);
    let stop = Arc::new(AtomicBool::new(false));
    let addr = serve_bsread(message.clone(), Arc::clone(&stop)).unwrap();
    assert_eq!(fetch_bsread(addr).unwrap(), message);
    stop.store(true, Ordering::Relaxed);
}

#[test]
fn basic256sha256_chunk_and_zmtp_bsread() {
    let key = [9u8; 32];
    let sig = [4u8; 32];
    let iv = [1u8; 16];
    let body = b"read-request".to_vec();
    let sealed = seal_secure_chunk(7, 3, 11, &body, &key, &sig, &iv).unwrap();
    assert!(sealed
        .windows(POLICY_BASIC256SHA256.len())
        .any(|w| w == POLICY_BASIC256SHA256.as_bytes()));
    assert!(!sealed.windows(body.len()).any(|w| w == body.as_slice()));
    let (token, sequence, request, plain) =
        caqtdm_datasource::open_secure_chunk(&sealed, &key, &sig).unwrap();
    assert_eq!((token, sequence, request, plain), (7, 3, 11, body));
    let mut tampered = sealed.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01;
    assert!(caqtdm_datasource::open_secure_chunk(&tampered, &key, &sig).is_err());

    let greet = greeting(false);
    assert!(parse_greeting(&greet).is_ok());
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, rx) = serve_zmtp(Arc::clone(&stop)).unwrap();
    let message = BsMessage {
        pulse_id: 9,
        channel: "CH".into(),
        value: 1.25,
    };
    send_bsread(addr, &message).unwrap();
    assert_eq!(recv_bsread(&rx).unwrap(), message);
    stop.store(true, Ordering::Relaxed);
}

#[test]
fn hipa_pro_and_sf_retrievers_round_trip() {
    use caqtdm_datasource::{
        apply_gmv, encode_gmv_reply, fetch_gmv, gmv_window, hipa_query, post_sf, pro_query,
        serve_gmv, serve_sf, sf_query, HIPA_HOST, PRO_HOST,
    };

    let (day, maxval) = gmv_window(3_600, 2);
    assert_eq!(day, 0);
    let hipa = hipa_query("MHP1", 26, 100, maxval);
    let pro = pro_query("MHP1", 26, 100, maxval);
    assert_eq!(hipa.host, HIPA_HOST);
    assert_eq!(pro.host, PRO_HOST);
    assert_eq!(hipa.logid, "U");
    assert_eq!(hipa.tau, -99);
    assert_ne!(hipa.host, pro.host);

    let mut values = vec![1.5f32, 2.5, 3.5, 9.0, 4.5, 5.5];
    let times = vec![10, 20, 30, 40, 50, 8];
    let errs = vec![0i16, 0, -1, 0, 1, 0];
    values.resize(maxval as usize, 0.0);
    let mut times = times;
    times.resize(maxval as usize, 0);
    let mut errs = errs;
    errs.resize(maxval as usize, 0);
    times[maxval as usize - 1] = 8;
    let samples = apply_gmv(&values, &times, &errs);
    assert_eq!(samples.len(), 6);
    assert_eq!(samples[2].value, 2.5);

    let reply = encode_gmv_reply(8, &values, &times, &errs);
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, rx) = serve_gmv(reply, Arc::clone(&stop)).unwrap();
    let got = fetch_gmv(addr, &pro).unwrap();
    assert_eq!(got[0].value, 1.5);
    assert_eq!(got[2].value, 2.5);
    let seen = rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
    assert_eq!(seen.host, PRO_HOST);
    assert_eq!(seen.device, "MHP1");
    stop.store(true, Ordering::Relaxed);

    let query = sf_query("SF:VAC", 1_700_000_000.0, 1_700_000_100.0, None, Some(2));
    assert!(query.contains("\"format\":\"json\""));
    assert!(query.contains("\"name\":\"SF:VAC\""));
    assert!(query.contains("\"nrOfBins\" : 2"));
    let body = r#"[{"channel":{"name":"SF:VAC","backend":"sf-archiverappliance"},"data":[{"globalSeconds":"1700000090","value":{"mean":1.25}},{"globalSeconds":"1700000095","value":{"mean":1.5}}]}]"#;
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, posted) = serve_sf(body.into(), Arc::clone(&stop)).unwrap();
    let samples = post_sf(addr, &query).unwrap();
    assert_eq!(samples.len(), 2);
    assert!((samples[0].y - 1.25).abs() < 1e-9);
    assert!(samples[0].x < 0.0);
    let sent = posted
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    assert!(sent.contains("SF:VAC"));
    stop.store(true, Ordering::Relaxed);
}

#[test]
fn opcua_open_secure_channel_derives_matching_keys() {
    use caqtdm_datasource::{
        client_finish, client_open, identity_named, open_secure_chunk, seal_secure_chunk,
        server_accept,
    };

    let client = identity_named("client").unwrap();
    let server = identity_named("server").unwrap();
    let nonce = b"client-nonce-32-bytes-pad-pad!!!";
    let server_nonce = b"server-nonce-32-bytes-pad-pad!!!";
    assert_eq!(nonce.len(), 32);
    let opn = client_open(&client, &server.cert_der, nonce).unwrap();
    assert!(opn.starts_with(b"OPNF"));
    assert!(!opn.windows(nonce.len()).any(|window| window == nonce));
    let (reply, server_keys) = server_accept(&server, &opn, server_nonce).unwrap();
    let client_keys = client_finish(&client, &reply, nonce).unwrap();
    assert_eq!(client_keys, server_keys);
    assert_eq!(client_keys.channel_id, 7);
    assert_eq!(client_keys.token_id, 3);
    let sealed = seal_secure_chunk(
        client_keys.token_id,
        1,
        1,
        b"read",
        &client_keys.client_enc,
        &client_keys.client_sign,
        &client_keys.client_iv,
    )
    .unwrap();
    let opened =
        open_secure_chunk(&sealed, &server_keys.client_enc, &server_keys.client_sign).unwrap();
    assert_eq!(opened.3, b"read");
    let mut tampered = opn.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    assert!(server_accept(&server, &tampered, server_nonce).is_err());
}

#[test]
fn zmtp_plain_and_curve_carry_bsread() {
    use caqtdm_datasource::{
        send_bsread_curve, send_bsread_plain, serve_curve, serve_plain, CurveIdentity,
    };

    let message = BsMessage {
        pulse_id: 11,
        channel: "CH".into(),
        value: 2.5,
    };
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, rx) = serve_plain("operator".into(), "beam".into(), Arc::clone(&stop)).unwrap();
    send_bsread_plain(addr, "operator", "beam", &message).unwrap();
    assert_eq!(recv_bsread(&rx).unwrap(), message);
    stop.store(true, Ordering::Relaxed);

    let stop = Arc::new(AtomicBool::new(false));
    let (addr, _rx) = serve_plain("operator".into(), "beam".into(), Arc::clone(&stop)).unwrap();
    assert!(send_bsread_plain(addr, "operator", "wrong", &message).is_err());
    stop.store(true, Ordering::Relaxed);

    let server = CurveIdentity::generate();
    let server_public = server.public.clone();
    let client = CurveIdentity::generate();
    let stop = Arc::new(AtomicBool::new(false));
    let (addr, rx) = serve_curve(server, Arc::clone(&stop)).unwrap();
    send_bsread_curve(addr, &server_public, &client, &message).unwrap();
    assert_eq!(recv_bsread(&rx).unwrap(), message);
    stop.store(true, Ordering::Relaxed);
}
