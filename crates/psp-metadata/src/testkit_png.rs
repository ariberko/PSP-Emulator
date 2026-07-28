//! A minimal PNG encoder for fixtures.
//!
//! [`tiny_png`](crate::testkit::tiny_png) emits only a signature and an IHDR,
//! which is enough to prove a parser extracted the right bytes but not enough for
//! a browser to draw. Fixture discs used to exercise the real UI need icons that
//! actually decode, so this writes complete, valid PNGs.
//!
//! Truecolour RGBA, no interlacing, one IDAT holding the zlib stream — the
//! simplest encoding that every decoder accepts.

/// Encodes 8-bit RGBA pixels as a PNG.
///
/// `pixels` must hold `width * height * 4` bytes in row-major order.
pub fn encode_rgba(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    assert_eq!(
        pixels.len(),
        width as usize * height as usize * 4,
        "pixel buffer does not match the declared dimensions"
    );

    let mut out = Vec::new();
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);

    // IHDR: 8-bit depth, colour type 6 (RGBA), deflate, no filter, no interlace.
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_chunk(&mut out, b"IHDR", &ihdr);

    // Each scanline is prefixed with its filter type; 0 means "none".
    let stride = width as usize * 4;
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for row in pixels.chunks(stride) {
        raw.push(0);
        raw.extend_from_slice(row);
    }

    // PNG's IDAT carries a zlib stream, not a bare deflate one.
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
    write_chunk(&mut out, b"IDAT", &compressed);
    write_chunk(&mut out, b"IEND", &[]);
    out
}

/// A 144×80 cover: diagonal gradient between two colours with a lighter band,
/// matching the dimensions of a real ICON0.
pub fn cover_art(from: [u8; 3], to: [u8; 3]) -> Vec<u8> {
    const WIDTH: u32 = 144;
    const HEIGHT: u32 = 80;
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            // Diagonal interpolation, so the gradient reads across the cover.
            let t = (x as f32 / WIDTH as f32) * 0.65 + (y as f32 / HEIGHT as f32) * 0.35;
            let mut rgb = [
                lerp(from[0], to[0], t),
                lerp(from[1], to[1], t),
                lerp(from[2], to[2], t),
            ];

            // A soft diagonal highlight, to make the art look deliberate.
            let band = ((x as f32 * 0.7 + y as f32 * 1.4) % 96.0 - 48.0).abs();
            if band < 9.0 {
                let strength = (1.0 - band / 9.0) * 0.28;
                for channel in &mut rgb {
                    *channel = lerp(*channel, 255, strength);
                }
            }

            pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }

    encode_rgba(WIDTH, HEIGHT, &pixels)
}

/// A 480×272 background, the dimensions of a real PIC1.
pub fn backdrop_art(from: [u8; 3], to: [u8; 3]) -> Vec<u8> {
    const WIDTH: u32 = 480;
    const HEIGHT: u32 = 272;
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let t = y as f32 / HEIGHT as f32;
            let wave = ((x as f32 / 60.0).sin() * 0.06) + 1.0;
            let rgb = [
                clamp_lerp(from[0], to[0], t * wave),
                clamp_lerp(from[1], to[1], t * wave),
                clamp_lerp(from[2], to[2], t * wave),
            ];
            pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }

    encode_rgba(WIDTH, HEIGHT, &pixels)
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

fn clamp_lerp(a: u8, b: u8, t: f32) -> u8 {
    lerp(a, b, t.clamp(0.0, 1.0))
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    // The CRC covers the chunk type and data, but not the length field.
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// CRC-32 as PNG specifies it (IEEE 802.3, reflected, `0xEDB88320`).
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            // Branch on the low bit: shift, and xor the polynomial when it was set.
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_known_check_value() {
        // The standard CRC-32 check value for "123456789".
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn writes_a_well_formed_png_header() {
        let png = encode_rgba(2, 2, &[0; 16]);
        assert_eq!(
            &png[0..8],
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
        );
        // Length 13, then "IHDR".
        assert_eq!(&png[8..12], &13u32.to_be_bytes());
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..20], &2u32.to_be_bytes());
        assert_eq!(&png[20..24], &2u32.to_be_bytes());
    }

    #[test]
    fn ends_with_an_iend_chunk() {
        let png = encode_rgba(1, 1, &[255, 0, 0, 255]);
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    #[test]
    fn the_idat_stream_round_trips_back_to_the_filtered_scanlines() {
        // Proves the zlib wrapper and filter bytes are right, which is what a
        // decoder actually needs.
        let pixels = [10u8, 20, 30, 255, 40, 50, 60, 255];
        let png = encode_rgba(2, 1, &pixels);

        let idat = find_chunk(&png, b"IDAT").expect("IDAT present");
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib(idat).expect("inflates");
        // One filter byte (0 = none) followed by the row.
        assert_eq!(raw[0], 0);
        assert_eq!(&raw[1..], &pixels);
    }

    #[test]
    fn cover_art_has_the_dimensions_of_a_real_icon0() {
        let png = cover_art([59, 110, 165], [18, 48, 74]);
        assert_eq!(&png[16..20], &144u32.to_be_bytes());
        assert_eq!(&png[20..24], &80u32.to_be_bytes());
    }

    #[test]
    fn backdrop_art_has_the_dimensions_of_a_real_pic1() {
        let png = backdrop_art([59, 110, 165], [18, 48, 74]);
        assert_eq!(&png[16..20], &480u32.to_be_bytes());
        assert_eq!(&png[20..24], &272u32.to_be_bytes());
    }

    #[test]
    #[should_panic(expected = "pixel buffer does not match")]
    fn rejects_a_pixel_buffer_of_the_wrong_size() {
        encode_rgba(4, 4, &[0; 8]);
    }

    /// Walks the chunk list looking for `kind`, returning its data.
    fn find_chunk<'a>(png: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
        let mut at = 8;
        while at + 12 <= png.len() {
            let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
            let this = &png[at + 4..at + 8];
            if this == kind {
                return Some(&png[at + 8..at + 8 + len]);
            }
            at += 12 + len;
        }
        None
    }
}
