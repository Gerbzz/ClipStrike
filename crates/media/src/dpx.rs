//! DPX (`.dpx`) stills, decoded clean-room from SMPTE ST 268 (Digital Picture Exchange).
//!
//! The first image element is decoded: luma, RGB, RGBA or ABGR, 8, 10, 12 or 16 bits per
//! component, big-endian (`SDPX`) or little-endian (`XPDS`) files. 10-bit components are filled into
//! 32-bit words (packing 1, method A: two padding bits at the bottom; packing 2, method B: at the
//! top), 12-bit components fill 16-bit words the same two ways. Scan lines start on a 32-bit
//! boundary and may carry end-of-line padding. Orientations 0–3 (either scan direction, either line
//! order) are honoured. Code values map linearly onto 8 bits (full range); a logarithmic (printing
//! density) transfer is shown as stored, without a film-log conversion.
//!
//! Not decoded (an "unsupported" error): run-length encoded elements, the 10/12-bit "packed"
//! layout (packing 0), Y'CbCr descriptors, 1-bit and floating-point components, transposed
//! orientations.

use crate::{MediaError, Result, still::Raster};

/// Generic file + image information header: everything we read lies below this.
const HEADER: usize = 1408;
/// First image element of the image information header (72 bytes each).
const ELEMENT: usize = 780;
/// Largest picture we decode (16384 × 16384): bounds the allocation a header can ask for.
const MAX_PIXELS: usize = 1 << 28;
/// "Undefined" value of numeric header fields.
const UNDEFINED: u32 = u32::MAX;

fn err(name: &str, what: &str) -> MediaError {
    MediaError::Decode(format!("{name}: DPX: {what}"))
}

fn unsupported(name: &str, what: String) -> MediaError {
    MediaError::Unsupported(format!("{name}: DPX {what}"))
}

/// Whether `bytes` starts with the DPX magic number in either byte order.
pub fn sniff(bytes: &[u8]) -> bool {
    bytes.starts_with(b"SDPX") || bytes.starts_with(b"XPDS")
}

/// Header and sample reads in the file's byte order.
#[derive(Clone, Copy)]
struct Order {
    big: bool,
}

impl Order {
    fn u16(self, b: &[u8], at: usize) -> u16 {
        let v = [b.get(at).copied().unwrap_or(0), b.get(at + 1).copied().unwrap_or(0)];
        if self.big { u16::from_be_bytes(v) } else { u16::from_le_bytes(v) }
    }

    fn u32(self, b: &[u8], at: usize) -> u32 {
        let v = core::array::from_fn(|i| b.get(at + i).copied().unwrap_or(0));
        if self.big { u32::from_be_bytes(v) } else { u32::from_le_bytes(v) }
    }
}

/// How components are stored in a scan line.
#[derive(Clone, Copy)]
enum Packing {
    /// One byte per component.
    Bytes,
    /// One 16-bit word per component; the value is `word >> shift` (12-bit method A: 4).
    Words { shift: u32 },
    /// Three 10-bit components per 32-bit word, the first in the high bits, above `pad` bits.
    Ten { pad: u32 },
}

impl Packing {
    /// Bytes of a scan line holding `comps` components, before end-of-line padding (lines start on
    /// a 32-bit boundary).
    fn line_bytes(self, comps: usize) -> Option<usize> {
        let raw = match self {
            Packing::Bytes => comps,
            Packing::Words { .. } => comps.checked_mul(2)?,
            Packing::Ten { .. } => comps.div_ceil(3).checked_mul(4)?,
        };
        raw.checked_next_multiple_of(4)
    }

    /// The `comps` component values of one scan line.
    fn unpack(self, line: &[u8], comps: usize, order: Order, out: &mut Vec<u16>) {
        out.clear();
        match self {
            Packing::Bytes => out.extend(line.iter().take(comps).map(|&b| u16::from(b))),
            Packing::Words { shift } => out.extend((0..comps).map(|i| order.u16(line, i * 2) >> shift)),
            Packing::Ten { pad } => {
                for w in 0..comps.div_ceil(3) {
                    let word = order.u32(line, w * 4);
                    for s in [20 + pad, 10 + pad, pad] {
                        if out.len() < comps {
                            out.push(((word >> s) & 0x3FF) as u16);
                        }
                    }
                }
            }
        }
    }
}

/// Decode the first image element of a DPX file to straight-alpha RGBA8, top row first.
pub fn decode(name: &str, bytes: &[u8]) -> Result<Raster> {
    let order = match bytes.get(..4) {
        Some(b"SDPX") => Order { big: true },
        Some(b"XPDS") => Order { big: false },
        _ => return Err(err(name, "not a DPX file")),
    };
    if bytes.len() < HEADER {
        return Err(err(name, "truncated header"));
    }
    let orientation = order.u16(bytes, 768);
    let (w, h) = (order.u32(bytes, 772) as usize, order.u32(bytes, 776) as usize);
    let el = ELEMENT;
    let descriptor = bytes.get(el + 20).copied().unwrap_or(0);
    let bits = bytes.get(el + 23).copied().unwrap_or(0);
    let packing = order.u16(bytes, el + 24);
    let encoding = order.u16(bytes, el + 26);
    let element_offset = order.u32(bytes, el + 28);
    let eol_padding = order.u32(bytes, el + 32);
    if w == 0 || h == 0 {
        return Err(err(name, "empty picture"));
    }
    let pixels = w.checked_mul(h).filter(|&n| n <= MAX_PIXELS).ok_or_else(|| err(name, "picture too large"))?;
    // component order per pixel: indices of R, G, B, A (None: opaque) in the stored components
    let (n, map): (usize, [Option<usize>; 4]) = match descriptor {
        6 => (1, [Some(0), Some(0), Some(0), None]),
        50 => (3, [Some(0), Some(1), Some(2), None]),
        51 => (4, [Some(0), Some(1), Some(2), Some(3)]),
        52 => (4, [Some(3), Some(2), Some(1), Some(0)]),
        d => return Err(unsupported(name, format!("descriptor {d}"))),
    };
    if encoding != 0 {
        return Err(unsupported(name, "run-length encoding".into()));
    }
    let layout = match (bits, packing) {
        (8, _) => Packing::Bytes,
        (16, _) => Packing::Words { shift: 0 },
        (10, 1) => Packing::Ten { pad: 2 },
        (10, 2) => Packing::Ten { pad: 0 },
        (12, 1) => Packing::Words { shift: 4 },
        (12, 2) => Packing::Words { shift: 0 },
        (b, p) => return Err(unsupported(name, format!("{b}-bit components with packing {p}"))),
    };
    let (flip_x, flip_y) = match orientation {
        0 => (false, false),
        1 => (true, false),
        2 => (false, true),
        3 => (true, true),
        o => return Err(unsupported(name, format!("orientation {o}"))),
    };
    let comps = w.checked_mul(n).ok_or_else(|| err(name, "picture too large"))?;
    let pad = if eol_padding == UNDEFINED { 0 } else { eol_padding as usize };
    let stride = layout.line_bytes(comps).and_then(|l| l.checked_add(pad)).ok_or_else(|| err(name, "picture too large"))?;
    let offset = match element_offset {
        0 | UNDEFINED => order.u32(bytes, 4) as usize,
        o => o as usize,
    };
    // the last line needs no end-of-line padding
    let size = stride.checked_mul(h - 1).and_then(|s| s.checked_add(stride - pad)).ok_or_else(|| err(name, "picture too large"))?;
    let data = offset.checked_add(size).and_then(|end| bytes.get(offset..end)).ok_or_else(|| err(name, "truncated image data"))?;
    let max = (1u32 << bits) - 1;
    let to8 = |v: u16| ((u32::from(v).min(max) * 255 + max / 2) / max) as u8;
    let mut rgba = vec![0u8; pixels * 4];
    let mut line = Vec::with_capacity(comps);
    for y in 0..h {
        let src = data.get(y * stride..).unwrap_or_default();
        layout.unpack(src, comps, order, &mut line);
        let dy = if flip_y { h - 1 - y } else { y };
        for x in 0..w {
            let dx = if flip_x { w - 1 - x } else { x };
            let o = (dy * w + dx) * 4;
            let Some(px) = rgba.get_mut(o..o + 4) else { continue };
            for (c, slot) in map.iter().enumerate() {
                let v = match slot {
                    Some(i) => line.get(x * n + i).map_or(0, |&v| to8(v)),
                    None => 255,
                };
                if let Some(d) = px.get_mut(c) {
                    *d = v;
                }
            }
        }
    }
    let has_alpha = map[3].is_some() && rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 255);
    Ok(Raster { width: w as u32, height: h as u32, rgba, has_alpha })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal DPX file (header + `data`) in the given byte order.
    fn file(big: bool, w: u32, h: u32, descriptor: u8, bits: u8, packing: u16, orientation: u16, data: &[u8]) -> Vec<u8> {
        let mut f = vec![0u8; 2048];
        let p32 = |f: &mut Vec<u8>, at: usize, v: u32| f[at..at + 4].copy_from_slice(&if big { v.to_be_bytes() } else { v.to_le_bytes() });
        let p16 = |f: &mut Vec<u8>, at: usize, v: u16| f[at..at + 2].copy_from_slice(&if big { v.to_be_bytes() } else { v.to_le_bytes() });
        f[0..4].copy_from_slice(if big { b"SDPX" } else { b"XPDS" });
        p32(&mut f, 4, 2048);
        p16(&mut f, 768, orientation);
        p16(&mut f, 770, 1);
        p32(&mut f, 772, w);
        p32(&mut f, 776, h);
        f[ELEMENT + 20] = descriptor;
        f[ELEMENT + 23] = bits;
        p16(&mut f, ELEMENT + 24, packing);
        p32(&mut f, ELEMENT + 28, UNDEFINED);
        p32(&mut f, ELEMENT + 32, UNDEFINED);
        f.extend_from_slice(data);
        f
    }

    #[test]
    fn ten_bit_method_a_and_b() {
        let word = |r: u32, g: u32, b: u32, pad: u32| ((r << (20 + pad)) | (g << (10 + pad)) | (b << pad)).to_be_bytes();
        let mut data = Vec::new();
        data.extend_from_slice(&word(1023, 0, 512, 2));
        data.extend_from_slice(&word(4, 1019, 0, 2));
        let r = decode("a.dpx", &file(true, 2, 1, 50, 10, 1, 0, &data)).unwrap();
        assert_eq!(r.rgba, [255, 0, 128, 255, 1, 254, 0, 255]);
        assert!(!r.has_alpha);
        let mut data = Vec::new();
        data.extend_from_slice(&word(1023, 0, 512, 0));
        data.extend_from_slice(&word(4, 1019, 0, 0));
        let r = decode("a.dpx", &file(true, 2, 1, 50, 10, 2, 0, &data)).unwrap();
        assert_eq!(r.rgba, [255, 0, 128, 255, 1, 254, 0, 255]);
    }

    #[test]
    fn rgba_components_roll_across_ten_bit_words() {
        // 2 RGBA pixels = 8 components = 3 words; the last word is half empty
        let comps = [1023u32, 0, 0, 1023, 0, 1023, 0, 0];
        let mut data = Vec::new();
        for c in comps.chunks(3) {
            let get = |i: usize| c.get(i).copied().unwrap_or(0);
            data.extend_from_slice(&((get(0) << 22) | (get(1) << 12) | (get(2) << 2)).to_le_bytes());
        }
        let r = decode("a.dpx", &file(false, 2, 1, 51, 10, 1, 0, &data)).unwrap();
        assert_eq!(r.rgba, [255, 0, 0, 255, 0, 255, 0, 0]);
        assert!(r.has_alpha);
    }

    #[test]
    fn eight_bit_lines_pad_to_words_and_orientation_flips() {
        // 1 × 2 RGB: each 3-byte line padded to 4; bottom-to-top, right-to-left orientation 3
        let r = decode("a.dpx", &file(false, 1, 2, 50, 8, 0, 3, &[1, 2, 3, 0, 4, 5, 6, 0])).unwrap();
        assert_eq!(r.rgba, [4, 5, 6, 255, 1, 2, 3, 255]);
        // 2 × 1 luma, 16-bit big-endian; ABGR, 12-bit method A
        let r = decode("a.dpx", &file(true, 2, 1, 6, 16, 0, 0, &[0xFF, 0xFF, 0x80, 0x00])).unwrap();
        assert_eq!(r.rgba, [255, 255, 255, 255, 128, 128, 128, 255]);
        let r = decode("a.dpx", &file(true, 1, 1, 52, 12, 1, 0, &[0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xF0])).unwrap();
        assert_eq!(r.rgba, [255, 0, 0, 128]);
    }

    #[test]
    fn hostile_headers_fail_cleanly() {
        // huge dimensions with no data, truncated data, unsupported layouts
        assert!(decode("a.dpx", &file(true, u32::MAX, u32::MAX, 50, 10, 1, 0, &[])).is_err());
        assert!(decode("a.dpx", &file(true, 4096, 4096, 50, 10, 1, 0, &[0; 64])).is_err());
        assert!(decode("a.dpx", &file(true, 2, 2, 50, 10, 1, 0, &[0; 12])).is_err());
        assert!(decode("a.dpx", &file(true, 1, 1, 100, 8, 0, 0, &[0; 4])).is_err());
        assert!(decode("a.dpx", &file(true, 1, 1, 50, 10, 0, 0, &[0; 4])).is_err());
        assert!(decode("a.dpx", &file(true, 1, 1, 50, 32, 0, 0, &[0; 16])).is_err());
        assert!(decode("a.dpx", &file(true, 1, 1, 50, 8, 0, 5, &[0; 4])).is_err());
        assert!(decode("a.dpx", &file(true, 0, 1, 50, 8, 0, 0, &[0; 4])).is_err());
        assert!(decode("a.dpx", b"SDPX").is_err());
        let mut f = file(true, 1, 1, 50, 8, 0, 0, &[0; 4]);
        f[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(decode("a.dpx", &f).is_err());
        let mut f = file(true, 1, 1, 50, 8, 0, 0, &[0; 4]);
        f[ELEMENT + 32..ELEMENT + 36].copy_from_slice(&(u32::MAX - 1).to_be_bytes());
        assert!(decode("a.dpx", &file(true, 1, 2, 50, 8, 0, 0, &[0; 4])).is_err());
        assert!(decode("a.dpx", &f).is_ok());
    }
}
