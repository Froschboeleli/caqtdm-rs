use caqtdm_core::PutValue;
use caqtdm_datasource::DataSource;
use caqtdm_epics_ca::{
    decode_header, encode_dbr, meta_size, pad_string, CaClient, CaHeader, CaSource, Record,
    SoftIoc, CA_MINOR, DBR_CTRL_DOUBLE, DBR_TIME_DOUBLE,
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
