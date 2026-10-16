use caqtdm_core::PutValue;
use caqtdm_datasource::DataSource;
use caqtdm_epics_ca::{
    decode_header, encode_dbr, meta_size, pad_string, CaClient, CaHeader, CaSource, Record,
    SoftIoc, CA_MINOR, DBR_CHAR, DBR_CTRL_DOUBLE, DBR_ENUM, DBR_FLOAT, DBR_LONG, DBR_SHORT,
    DBR_STRING, DBR_TIME_DOUBLE,
};

#[test]
fn header_extension_and_padding() {
    let mut header = CaHeader::new(1);
    header.count = 100_000;
    header.payload_size = 80_000;
    let bytes = header.encode();
    assert_eq!(bytes.len(), 24);
    let (decoded, n) = decode_header(&bytes).unwrap();
    assert_eq!(n, 24);
    assert_eq!(decoded.count, 100_000);
    assert_eq!(decoded.payload_size, 80_000);
    assert_eq!(pad_string("abc").len(), 8);
    assert_eq!(meta_size(DBR_CTRL_DOUBLE), 80);
    assert_eq!(meta_size(DBR_TIME_DOUBLE), 16);
    let record = Record::new("X", 1.5);
    let payload = encode_dbr(&record, DBR_CTRL_DOUBLE, 1).unwrap();
    assert_eq!(payload.len(), 88);
    let _ = CA_MINOR;
}

#[test]
fn search_read_write_and_monitor() {
    let mut record = Record::new("SR:VAC:P", 1.25);
    record.units = "mbar".into();
    record.precision = 2;
    let ioc = SoftIoc::start(vec![record]).unwrap();
    let tcp = CaClient::search(ioc.udp, "SR:VAC:P").unwrap();
    assert_eq!(tcp.port(), ioc.tcp.port());

    let mut client = CaClient::connect(tcp).unwrap();
    client.create("SR:VAC:P").unwrap();
    let data = client.read("SR:VAC:P").unwrap();
    assert!((data.value.as_f64().unwrap() - 1.25).abs() < 1e-9);
    assert_eq!(data.units, "mbar");
    assert_eq!(data.precision, 2);
    assert!((data.limits.upper_display - 100.0).abs() < 1e-9);

    let sub = client.subscribe("SR:VAC:P").unwrap();
    let mut saw = false;
    for _ in 0..20 {
        for event in client.poll().unwrap() {
            if event.sub_id == sub && (event.data.value.as_f64().unwrap() - 1.25).abs() < 1e-6 {
                saw = true;
            }
        }
        if saw {
            break;
        }
    }
    assert!(saw, "initial monitor event");

    client.write_double("SR:VAC:P", 3.5).unwrap();
    let data = client.read("SR:VAC:P").unwrap();
    assert!((data.value.as_f64().unwrap() - 3.5).abs() < 1e-9);

    let mut source = CaSource::connect(ioc.tcp).unwrap();
    let spec = caqtdm_core::parse_channel("SR:VAC:P", "epics3");
    let id = source.add_monitor(&spec).unwrap();
    source.set_value(&spec, &PutValue::Double(4.0)).unwrap();
    let mut found = false;
    for _ in 0..20 {
        for update in source.poll(0) {
            if update.id == id && (update.data.value.as_f64().unwrap() - 4.0).abs() < 1e-6 {
                found = true;
            }
        }
        if found {
            break;
        }
    }
    assert!(found, "datasource saw the write");
}

#[test]
fn soft_ioc_serves_every_native_dbr() {
    let mut shorts = Record::new("SR:BITS", 0.0);
    shorts.native = DBR_SHORT;
    shorts.waveform = vec![10.0, 20.0, 30.0];
    shorts.units = "bit".into();
    let mut text = Record::new("SR:NAME", 0.0);
    text.native = DBR_STRING;
    text.text = "pump".into();
    let mut mode = Record::new("SR:MODE", 1.0);
    mode.native = DBR_ENUM;
    mode.enum_strings = vec!["off".into(), "on".into()];
    let mut byte = Record::new("SR:BYTE", 0.0);
    byte.native = DBR_CHAR;
    byte.waveform = vec![1.0, 255.0];
    let mut flt = Record::new("SR:F", 1.5);
    flt.native = DBR_FLOAT;
    let mut lng = Record::new("SR:L", 100_000.0);
    lng.native = DBR_LONG;

    let ioc = SoftIoc::start(vec![shorts, text, mode, byte, flt, lng]).unwrap();
    let mut client = CaClient::connect(ioc.tcp).unwrap();
    for name in ["SR:BITS", "SR:NAME", "SR:MODE", "SR:BYTE", "SR:F", "SR:L"] {
        client.create(name).unwrap();
    }

    let bits = client.read_dbr("SR:BITS", DBR_SHORT + 21, 3).unwrap();
    assert_eq!(bits.value.as_f64_slice(), vec![10.0, 20.0, 30.0]);
    assert_eq!(bits.units, "bit");
    client
        .write_values("SR:BITS", DBR_SHORT, &[11.0, 22.0, 33.0])
        .unwrap();
    let bits = client.read_dbr("SR:BITS", DBR_SHORT + 14, 3).unwrap();
    assert_eq!(bits.value.as_f64_slice(), vec![11.0, 22.0, 33.0]);

    let name = client.read_dbr("SR:NAME", DBR_STRING + 7, 1).unwrap();
    assert_eq!(name.value.as_text(), "pump");
    client.write_text("SR:NAME", "valve").unwrap();
    let name = client.read_dbr("SR:NAME", DBR_STRING, 1).unwrap();
    assert_eq!(name.value.as_text(), "valve");

    let mode = client.read_dbr("SR:MODE", DBR_ENUM + 28, 1).unwrap();
    assert_eq!(mode.enum_strings, ["off".to_string(), "on".into()]);
    assert_eq!(mode.value.as_i64(), Some(1));
    client.write_values("SR:MODE", DBR_ENUM, &[0.0]).unwrap();
    let mode = client.read_dbr("SR:MODE", DBR_ENUM, 1).unwrap();
    assert_eq!(mode.value.as_i64(), Some(0));

    let byte = client.read_dbr("SR:BYTE", DBR_CHAR, 2).unwrap();
    assert_eq!(byte.value.as_f64_slice(), vec![1.0, 255.0]);
    let flt = client.read_dbr("SR:F", DBR_FLOAT + 28, 1).unwrap();
    assert!((flt.value.as_f64().unwrap() - 1.5).abs() < 1e-5);
    let lng = client.read_dbr("SR:L", DBR_LONG, 1).unwrap();
    assert_eq!(lng.value.as_i64(), Some(100_000));

    let sub = client.subscribe_dbr("SR:L", DBR_LONG + 14, 1).unwrap();
    client.write_values("SR:L", DBR_LONG, &[7.0]).unwrap();
    let mut saw = false;
    for _ in 0..20 {
        for event in client.poll().unwrap() {
            if event.sub_id == sub && event.data.value.as_i64() == Some(7) {
                saw = true;
            }
        }
        if saw {
            break;
        }
    }
    assert!(saw, "long monitor followed the write");
}
