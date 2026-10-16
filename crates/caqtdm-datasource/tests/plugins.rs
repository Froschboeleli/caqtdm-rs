use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use caqtdm_core::PutValue;
use caqtdm_datasource::{
    build_archive_url, decode_bsread, decode_response, encode_bsread, encode_hello, encode_node_id,
    encode_read_request, encode_request, exchange, fetch_archiver, fetch_bsread, parse_fixture,
    read_node, samples_to_json, serve_archiver, serve_bsread, serve_loopback, serve_read,
    BsMessage, BsreadSource, DataSource, EnvSource, GpsSource, Hub, InternSource, Mbap,
    ModbusSource, NodeId, OpcuaSource, Pdu, RegisterMap, SimSource, READ_REQUEST, VARIANT_DOUBLE,
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
