//! Apple CgBI PNG writer.
//!
//! The form Xcode's `--compress-pngs` produces for device builds: a `CgBI` chunk
//! after the signature, BGRA rows with premultiplied alpha, and an IDAT holding a raw
//! deflate stream (no zlib header, no adler32).

use flate2::Compression;
use flate2::write::DeflateEncoder;
use std::io::{self, Write};

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

// CGBitmapInfo as Apple's actool writes it for RGBA icons; taken from the reference sample.
const CGBI_DATA: [u8; 4] = [0x50, 0x00, 0x20, 0x06];

fn write_chunk(out: &mut impl Write, kind: &[u8; 4], data: &[u8]) -> io::Result<()> {
    let len = u32::try_from(data.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "PNG chunk over 4 GiB"))?;
    out.write_all(&len.to_be_bytes())?;
    out.write_all(kind)?;
    out.write_all(data)?;
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    crc.update(data);
    out.write_all(&crc.finalize().to_be_bytes())
}

pub fn write_cgbi_png(rgba: &image::RgbaImage, out: &mut impl Write) -> io::Result<()> {
    let (w, h) = rgba.dimensions();

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);

    let row_len = w as usize * 4;
    let mut raw = Vec::with_capacity((row_len + 1) * h as usize);
    for row in rgba.as_raw().chunks_exact(row_len.max(1)) {
        raw.push(0);
        for px in row.as_chunks::<4>().0 {
            let a = u32::from(px[3]);
            let premul = |c: u8| ((u32::from(c) * a + 127) / 255) as u8;
            raw.extend_from_slice(&[premul(px[2]), premul(px[1]), premul(px[0]), px[3]]);
        }
    }

    let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
    enc.write_all(&raw)?;
    let idat = enc.finish()?;

    out.write_all(&SIGNATURE)?;
    write_chunk(out, b"CgBI", &CGBI_DATA)?;
    write_chunk(out, b"IHDR", &ihdr)?;
    write_chunk(out, b"IDAT", &idat)?;
    write_chunk(out, b"IEND", &[])
}
