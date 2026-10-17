//! Rank-2 MDA files, the subset `mdaReader_gimmeYerData` reads.
//!
//! The layout follows Argonne `mda_load` (XDR, big-endian). Every integer,
//! including the 16-bit fields, is a 4-byte word. A counted string is the
//! length, then `xdr_string` (a second length, the bytes, and pad to 4). An
//! empty string is a single zero length. A 2D image is the detector column of
//! each rank-1 subscan. The detector number is `atol` of the PV's last four
//! characters, minus one. A path that starts with `//` is read from `/net`
//! plus that path, matching `mdaReaderThread`.

/// `//server/...` becomes `/net//server/...`.
pub fn mda_filename(path: &str) -> String {
    if path.starts_with("//") {
        format!("/net{path}")
    } else {
        path.to_string()
    }
}

/// Write a rank-2 file whose subscans hold one detector column each.
pub fn write_rank2(rows: &[Vec<f32>], detector: i16, scan_name: &str) -> Vec<u8> {
    let ny = rows.len() as i32;
    let nx = rows.first().map(|row| row.len()).unwrap_or(0) as i32;
    let subs: Vec<Vec<u8>> = rows
        .iter()
        .map(|row| encode_subscan(row, detector))
        .collect();

    let mut file = Xdr::new();
    file.f32(1.3);
    file.i32(1);
    file.i16(2);
    file.i32(ny);
    file.i32(nx);
    file.i16(1);
    file.i32(0);

    file.i16(2);
    file.i32(ny);
    file.i32(ny);
    let offset_at = file.len();
    for _ in 0..ny.max(0) {
        file.i32(0);
    }
    file.counted(scan_name);
    file.counted("");
    file.i16(0);
    file.i16(0);
    file.i16(0);

    let mut offsets = Vec::with_capacity(subs.len());
    for sub in &subs {
        offsets.push(file.len() as i32);
        file.bytes.extend_from_slice(sub);
    }
    for (index, offset) in offsets.iter().enumerate() {
        let at = offset_at + index * 4;
        file.bytes[at..at + 4].copy_from_slice(&offset.to_be_bytes());
    }
    file.bytes
}

/// Detector image for `pv`, `nx` by `ny`, in row-major order.
///
/// This is the branch `gimmeYerData` takes when the top scan name is not a
/// prefix of the PV. Missing points are 0.
pub fn detector_image(bytes: &[u8], pv: &str, nx: usize, ny: usize) -> Result<Vec<f32>, String> {
    let det = detector_number(pv).ok_or_else(|| format!("detector number in {pv}"))?;
    let mut reader = Reader::new(bytes);
    read_header(&mut reader)?;
    let scan = read_scan(&mut reader)?;
    if pv.starts_with(&scan.name) {
        return Err(format!("scan name {} prefixes {pv}", scan.name));
    }
    if scan.rank < 2 {
        return Err("scan rank".into());
    }
    let mut image = vec![0.0_f32; nx.saturating_mul(ny)];
    let rows = ny.min(scan.last_point.max(0) as usize);
    for row in 0..rows {
        let Some(sub) = scan.subs.get(row).and_then(|item| item.as_ref()) else {
            continue;
        };
        let Some(index) = sub
            .detectors
            .iter()
            .position(|number| i32::from(*number) == det)
        else {
            continue;
        };
        let samples = sub.data.get(index).map(Vec::as_slice).unwrap_or(&[]);
        let cols = nx.min(sub.last_point.max(0) as usize).min(samples.len());
        for col in 0..cols {
            image[row * nx + col] = samples[col];
        }
    }
    Ok(image)
}

fn detector_number(pv: &str) -> Option<i32> {
    let start = pv.len().saturating_sub(4);
    let tail = pv.get(start..)?;
    let number = atol(tail) - 1;
    if number < 0 {
        None
    } else {
        Some(number as i32)
    }
}

fn atol(text: &str) -> i64 {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    let start = index;
    if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
        index += 1;
    }
    let digits = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == digits {
        0
    } else {
        text[start..index].parse().unwrap_or(0)
    }
}

fn encode_subscan(samples: &[f32], detector: i16) -> Vec<u8> {
    let mut scan = Xdr::new();
    let count = samples.len() as i32;
    scan.i16(1);
    scan.i32(count);
    scan.i32(count);
    scan.counted("inner");
    scan.counted("");
    scan.i16(0);
    scan.i16(1);
    scan.i16(0);
    scan.i16(detector);
    scan.counted("det");
    scan.counted("");
    scan.counted("");
    for sample in samples {
        scan.f32(*sample);
    }
    scan.bytes
}

struct Scan {
    rank: i16,
    last_point: i32,
    name: String,
    detectors: Vec<i16>,
    data: Vec<Vec<f32>>,
    subs: Vec<Option<Scan>>,
}

fn read_header(reader: &mut Reader<'_>) -> Result<(), String> {
    let _version = reader.f32()?;
    let _scan = reader.i32()?;
    let rank = reader.i16()?;
    if rank < 0 {
        return Err("header rank".into());
    }
    for _ in 0..rank {
        let _dim = reader.i32()?;
    }
    let _regular = reader.i16()?;
    let _extra = reader.i32()?;
    Ok(())
}

fn read_scan(reader: &mut Reader<'_>) -> Result<Scan, String> {
    let rank = reader.i16()?;
    let requested = reader.i32()?;
    let last_point = reader.i32()?;
    if rank < 1 || requested < 0 || last_point < 0 {
        return Err("scan header".into());
    }
    let mut offsets = Vec::new();
    if rank > 1 {
        for _ in 0..requested {
            offsets.push(reader.i32()?);
        }
        for offset in offsets.iter().take(last_point as usize) {
            if *offset == 0 {
                return Err("zero scan offset".into());
            }
        }
    }
    let name = reader.counted()?;
    let _time = reader.counted()?;
    let npos = reader.i16()?;
    let ndet = reader.i16()?;
    let ntrig = reader.i16()?;
    if npos < 0 || ndet < 0 || ntrig < 0 {
        return Err("scan counts".into());
    }
    for _ in 0..npos {
        let _number = reader.i16()?;
        for _ in 0..7 {
            let _text = reader.counted()?;
        }
    }
    let mut detectors = Vec::with_capacity(ndet as usize);
    for _ in 0..ndet {
        detectors.push(reader.i16()?);
        for _ in 0..3 {
            let _text = reader.counted()?;
        }
    }
    for _ in 0..ntrig {
        let _number = reader.i16()?;
        let _name = reader.counted()?;
        let _command = reader.f32()?;
    }
    for _ in 0..npos {
        for _ in 0..requested {
            let _sample = reader.f64()?;
        }
    }
    let mut data = Vec::with_capacity(ndet as usize);
    for _ in 0..ndet {
        let mut column = Vec::with_capacity(requested as usize);
        for _ in 0..requested {
            column.push(reader.f32()?);
        }
        data.push(column);
    }
    let mut subs = Vec::new();
    if rank > 1 {
        for offset in offsets {
            if offset == 0 {
                subs.push(None);
                continue;
            }
            reader.seek(offset as usize)?;
            subs.push(Some(read_scan(reader)?));
        }
    }
    Ok(Scan {
        rank,
        last_point,
        name,
        detectors,
        data,
        subs,
    })
}

struct Xdr {
    bytes: Vec<u8>,
}

impl Xdr {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn i16(&mut self, value: i16) {
        self.i32(i32::from(value));
    }

    fn f32(&mut self, value: f32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn counted(&mut self, text: &str) {
        let raw = text.as_bytes();
        self.i32(raw.len() as i32);
        if raw.is_empty() {
            return;
        }
        self.i32(raw.len() as i32);
        self.bytes.extend_from_slice(raw);
        let pad = (4 - raw.len() % 4) % 4;
        self.bytes.extend(std::iter::repeat_n(0, pad));
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    index: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, index: 0 }
    }

    fn seek(&mut self, index: usize) -> Result<(), String> {
        if index > self.bytes.len() {
            return Err("seek".into());
        }
        self.index = index;
        Ok(())
    }

    fn take(&mut self, size: usize) -> Result<&'a [u8], String> {
        let end = self
            .index
            .checked_add(size)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| "truncated mda".to_string())?;
        let slice = &self.bytes[self.index..end];
        self.index = end;
        Ok(slice)
    }

    fn i32(&mut self) -> Result<i32, String> {
        let bytes = self.take(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn i16(&mut self) -> Result<i16, String> {
        Ok(self.i32()? as i16)
    }

    fn f32(&mut self) -> Result<f32, String> {
        let bytes = self.take(4)?;
        Ok(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn f64(&mut self) -> Result<f64, String> {
        let bytes = self.take(8)?;
        Ok(f64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn counted(&mut self) -> Result<String, String> {
        let length = self.i32()?;
        if length == 0 {
            return Ok(String::new());
        }
        if length < 0 {
            return Err("string length".into());
        }
        let size = self.i32()?;
        if size < 0 || size > length {
            return Err("string size".into());
        }
        let raw = self.take(size as usize)?.to_vec();
        let pad = (4 - (size as usize) % 4) % 4;
        let _ = self.take(pad)?;
        Ok(String::from_utf8_lossy(&raw).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank2_round_trip_matches_gimme_yer_data() {
        assert_eq!(mda_filename("//ioc/scan.mda"), "/net//ioc/scan.mda");
        assert_eq!(detector_number("det0001"), Some(0));
        let bytes = write_rank2(&[vec![1.0, 0.0], vec![0.0, 4.0]], 0, "outer");
        let image = detector_image(&bytes, "13idc:det0001", 2, 2).unwrap();
        assert_eq!(image, vec![1.0, 0.0, 0.0, 4.0]);
        assert!(detector_image(&bytes, "outer0001", 2, 2).is_err());
    }
}
