use super::EntryError;
use std::path::Path;

use crc::{Algorithm, Crc, CRC_32_ISO_HDLC};

// CRC_32_ISO_HDLC is compatible with Python 3
const CRC32_ALGORITHM: Algorithm<u32> = CRC_32_ISO_HDLC;

/// Returns a checksum calculated with CRC32 using the ISO HDLC algorithm for compatibility with Python.
pub fn crc32_iso_hdlc_checksum(bytes: &[u8]) -> u32 {
    let crc: Crc<u32> = Crc::<u32>::new(&CRC32_ALGORITHM);
    crc.checksum(bytes)
}

// from:
// +entries: [ { .. }, { .. } ]

// to:
// entries: [
//  { n: N, v: [ { .. }, { .. } ] }
// ]

// from:
// +entries: [ { .. }, { .. } ]

// to:
// entries: [
//  { n: N, v: [ { .. }, { .. } ] },
//  { n: N, v: [ { .. }, { .. } ] }
// ]

// replace `+entries` with new `entries`
