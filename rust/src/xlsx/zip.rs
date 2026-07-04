//! Minimal deterministic ZIP writer (STORE) + reader (STORE/DEFLATE).
//! XLSX is a ZIP of XML parts. Port of `js/src/xlsx/zip.js`.

use flate2::read::DeflateDecoder;
use std::io::Read;

const CRC_TABLE: [u32; 256] = build_crc_table();

const fn build_crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut n = 0usize;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xedb88320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[n] = c;
        n += 1;
    }
    t
}

pub fn crc32(buf: &[u8]) -> u32 {
    let mut c: u32 = 0xffffffff;
    for &b in buf {
        c = CRC_TABLE[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
    }
    c ^ 0xffffffff
}

pub struct ZipEntry {
    pub name: String,
    pub data: Vec<u8>,
}

/// Entries emitted in order with a fixed DOS timestamp (1980-01-01), STORE
/// method, so output is byte-stable.
pub fn zip_write(entries: &[ZipEntry]) -> Vec<u8> {
    let mut locals: Vec<u8> = Vec::new();
    let mut centrals: Vec<u8> = Vec::new();
    let mut offset: u32 = 0;
    for e in entries {
        let name_buf = e.name.as_bytes();
        let body = &e.data;
        let crc = crc32(body);
        let mut local = Vec::with_capacity(30);
        local.extend_from_slice(&0x04034b50u32.to_le_bytes());
        local.extend_from_slice(&20u16.to_le_bytes()); // version needed
        local.extend_from_slice(&0u16.to_le_bytes()); // flags
        local.extend_from_slice(&0u16.to_le_bytes()); // method: STORE
        local.extend_from_slice(&0u16.to_le_bytes()); // dos time
        local.extend_from_slice(&0x21u16.to_le_bytes()); // dos date: 1980-01-01
        local.extend_from_slice(&crc.to_le_bytes());
        local.extend_from_slice(&(body.len() as u32).to_le_bytes());
        local.extend_from_slice(&(body.len() as u32).to_le_bytes());
        local.extend_from_slice(&(name_buf.len() as u16).to_le_bytes());
        local.extend_from_slice(&0u16.to_le_bytes());
        locals.extend_from_slice(&local);
        locals.extend_from_slice(name_buf);
        locals.extend_from_slice(body);

        let mut central = Vec::with_capacity(46);
        central.extend_from_slice(&0x02014b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0x21u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(body.len() as u32).to_le_bytes());
        central.extend_from_slice(&(body.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name_buf.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra len
        central.extend_from_slice(&0u16.to_le_bytes()); // comment len
        central.extend_from_slice(&0u16.to_le_bytes()); // disk number
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&offset.to_le_bytes());
        centrals.extend_from_slice(&central);
        centrals.extend_from_slice(name_buf);
        offset += 30 + name_buf.len() as u32 + body.len() as u32;
    }
    let mut out = locals;
    let central_len = centrals.len() as u32;
    let central_offset = offset;
    out.extend_from_slice(&centrals);
    let mut eocd = Vec::with_capacity(22);
    eocd.extend_from_slice(&0x06054b50u32.to_le_bytes());
    eocd.extend_from_slice(&0u16.to_le_bytes()); // disk
    eocd.extend_from_slice(&0u16.to_le_bytes()); // cd disk
    eocd.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    eocd.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    eocd.extend_from_slice(&central_len.to_le_bytes());
    eocd.extend_from_slice(&central_offset.to_le_bytes());
    eocd.extend_from_slice(&0u16.to_le_bytes()); // comment len
    out.extend_from_slice(&eocd);
    out
}

fn read_u16(buf: &[u8], p: usize) -> Option<u16> {
    Some(u16::from_le_bytes(buf.get(p..p + 2)?.try_into().ok()?))
}
fn read_u32(buf: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_le_bytes(buf.get(p..p + 4)?.try_into().ok()?))
}

/// Read a ZIP into ordered `(name, bytes)` entries, verifying CRCs.
pub fn zip_read(buf: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    if buf.len() < 22 {
        return Err("not a zip: too small".to_string());
    }
    let mut eocd: i64 = -1;
    let mut i = buf.len() as i64 - 22;
    while i >= 0 {
        if read_u32(buf, i as usize) == Some(0x06054b50) {
            eocd = i;
            break;
        }
        i -= 1;
    }
    if eocd == -1 {
        return Err("not a zip: EOCD missing".to_string());
    }
    let eocd = eocd as usize;
    let count = read_u16(buf, eocd + 10).ok_or("bad EOCD")?;
    let mut p = read_u32(buf, eocd + 16).ok_or("bad EOCD")? as usize;
    let mut out = Vec::new();
    for _ in 0..count {
        if read_u32(buf, p) != Some(0x02014b50) {
            return Err("bad central header".to_string());
        }
        let method = read_u16(buf, p + 10).ok_or("truncated central header")?;
        let crc = read_u32(buf, p + 16).ok_or("truncated central header")?;
        let csize = read_u32(buf, p + 20).ok_or("truncated central header")? as usize;
        let name_len = read_u16(buf, p + 28).ok_or("truncated central header")? as usize;
        let extra_len = read_u16(buf, p + 30).ok_or("truncated central header")? as usize;
        let comment_len = read_u16(buf, p + 32).ok_or("truncated central header")? as usize;
        let local_off = read_u32(buf, p + 42).ok_or("truncated central header")? as usize;
        let name = String::from_utf8_lossy(
            buf.get(p + 46..p + 46 + name_len)
                .ok_or("truncated name")?,
        )
        .into_owned();
        let l_name_len = read_u16(buf, local_off + 26).ok_or("bad local header")? as usize;
        let l_extra_len = read_u16(buf, local_off + 28).ok_or("bad local header")? as usize;
        let data_start = local_off + 30 + l_name_len + l_extra_len;
        let raw = buf
            .get(data_start..data_start + csize)
            .ok_or("truncated entry data")?;
        let data = match method {
            0 => raw.to_vec(),
            8 => {
                let mut decoder = DeflateDecoder::new(raw);
                let mut out = Vec::new();
                decoder
                    .read_to_end(&mut out)
                    .map_err(|e| format!("inflate failed for {name}: {e}"))?;
                out
            }
            m => return Err(format!("unsupported zip method {m} for {name}")),
        };
        if crc32(&data) != crc {
            return Err(format!("crc mismatch: {name}"));
        }
        out.push((name, data));
        p += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}
