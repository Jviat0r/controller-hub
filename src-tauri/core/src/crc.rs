//! CRC-32 (IEEE 802.3, reflected) with a compile-time table. Sony pads
//! require it on every Bluetooth output report; a wrong CRC is silently
//! dropped by the controller, so this is tested against the standard vector.

const fn make_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

static TABLE: [u32; 256] = make_table();

fn update(mut crc: u32, data: &[u8]) -> u32 {
    for &b in data {
        crc = TABLE[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc
}

pub fn crc32(data: &[u8]) -> u32 {
    update(0xFFFF_FFFF, data) ^ 0xFFFF_FFFF
}

/// Bluetooth HID output transactions are CRC'd over the transaction header
/// byte 0xA2 followed by the report bytes (report id included).
pub fn bt_output_crc(report_without_crc: &[u8]) -> u32 {
    update(update(0xFFFF_FFFF, &[0xA2]), report_without_crc) ^ 0xFFFF_FFFF
}

/// Fill the last four bytes of `buf` with the little-endian CRC of the rest.
pub fn append_bt_crc(buf: &mut [u8]) {
    let n = buf.len();
    assert!(n > 4, "report too short for a CRC");
    let crc = bt_output_crc(&buf[..n - 4]);
    buf[n - 4..].copy_from_slice(&crc.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn bt_crc_includes_the_a2_header() {
        let body = [0x11u8, 0xC4, 0x00];
        let mut with_header = vec![0xA2];
        with_header.extend_from_slice(&body);
        assert_eq!(bt_output_crc(&body), crc32(&with_header));
    }

    #[test]
    fn append_writes_little_endian_tail() {
        let mut buf = vec![0u8; 8];
        buf[0] = 0x31;
        append_bt_crc(&mut buf);
        let expect = bt_output_crc(&buf[..4]).to_le_bytes();
        assert_eq!(&buf[4..], &expect);
    }
}
