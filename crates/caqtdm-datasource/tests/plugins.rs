use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use caqtdm_core::PutValue;
use caqtdm_datasource::{
    build_archive_url, decode_response, encode_request, exchange, parse_fixture, serve_loopback,
    BsreadSource, DataSource, EnvSource, GpsSource, Hub, InternSource, Mbap, ModbusSource,
    OpcuaSource, Pdu, RegisterMap, SimSource,
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

    let mut bs = BsreadSource::new();
    let spec = caqtdm_core::parse_channel("pulse_id", "bsread");
    bs.add_monitor(&spec).unwrap();
    let a = bs.poll(0);
    let b = bs.poll(50);
    assert!(b[0].data.value.as_i64().unwrap() > a[0].data.value.as_i64().unwrap());
}
