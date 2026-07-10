//! Splat export formats (ROADMAP-V2 1.6).
//!
//! PLY stays the default. `.splat` is the 32-byte-per-Gaussian layout the
//! common web viewers read, and `.spz` is Niantic's gzip-compressed format
//! (container version 2).

use super::{ply, SplatCloud, SH_C0};
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Ply,
    Splat,
    Spz,
}

impl Format {
    pub fn parse(s: &str) -> Option<Format> {
        Some(match s.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "ply" => Format::Ply,
            "splat" => Format::Splat,
            "spz" => Format::Spz,
            _ => return None,
        })
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Ply => "ply",
            Format::Splat => "splat",
            Format::Spz => "spz",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Ply => "Gaussian Splat PLY",
            Format::Splat => "Web splat",
            Format::Spz => "Niantic SPZ",
        }
    }

    /// Infer from a destination path, falling back to PLY.
    pub fn from_path(p: &Path) -> Format {
        p.extension()
            .and_then(|e| e.to_str())
            .and_then(Format::parse)
            .unwrap_or(Format::Ply)
    }
}

fn to_u8(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Write `cloud` to `path` in the format implied by its extension.
pub fn write(path: &Path, cloud: &SplatCloud, format: Format) -> Result<(), String> {
    match format {
        Format::Ply => ply::write_ply(path, cloud),
        Format::Splat => {
            let bytes = encode_splat(cloud);
            std::fs::write(path, bytes).map_err(|e| format!("Cannot write {}: {e}", path.display()))
        }
        Format::Spz => {
            let bytes = encode_spz(cloud)?;
            std::fs::write(path, bytes).map_err(|e| format!("Cannot write {}: {e}", path.display()))
        }
    }
}

// ---- .splat ----------------------------------------------------------------

/// 32 bytes per Gaussian: `f32[3]` position, `f32[3]` linear scale,
/// `u8[4]` RGBA, `u8[4]` quaternion `(w, x, y, z)` mapped through `q*128+128`.
pub fn encode_splat(cloud: &SplatCloud) -> Vec<u8> {
    let mut out = Vec::with_capacity(cloud.len() * 32);
    for i in 0..cloud.len() {
        for k in 0..3 {
            out.extend_from_slice(&cloud.positions[i][k].to_le_bytes());
        }
        let s = cloud.scale(i);
        for k in 0..3 {
            out.extend_from_slice(&s[k].to_le_bytes());
        }
        let c = cloud.sh_dc[i];
        out.push(to_u8((0.5 + SH_C0 * c[0]) * 255.0));
        out.push(to_u8((0.5 + SH_C0 * c[1]) * 255.0));
        out.push(to_u8((0.5 + SH_C0 * c[2]) * 255.0));
        out.push(to_u8(cloud.opacity(i) * 255.0));
        let q = cloud.unit_rot(i);
        for k in 0..4 {
            out.push(to_u8(q[k] * 128.0 + 128.0));
        }
    }
    out
}

// ---- .spz ------------------------------------------------------------------

const SPZ_MAGIC: u32 = 0x5053_474e; // "NGSP"
const SPZ_VERSION: u32 = 2;
const SPZ_FRACTIONAL_BITS: u8 = 12;
/// Niantic's fixed colour scale for the DC band.
const SPZ_COLOR_SCALE: f32 = 0.15;

/// Round `x * 128 + 128` onto a bucket grid, as SPZ does for SH coefficients.
fn quantize_sh(x: f32, bucket: i32) -> u8 {
    let q = (x * 128.0).round() as i32 + 128;
    let q = (q + bucket / 2) / bucket * bucket;
    q.clamp(0, 255) as u8
}

/// Encode to SPZ container version 2 (gzip-compressed).
pub fn encode_spz(cloud: &SplatCloud) -> Result<Vec<u8>, String> {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let n = cloud.len();
    if n > u32::MAX as usize {
        return Err("Too many splats for the SPZ format.".into());
    }
    let sh_degree = cloud.sh_degree();
    let k = cloud.rest_per_channel;

    let mut raw: Vec<u8> = Vec::with_capacity(16 + n * (9 + 1 + 3 + 3 + 3 + k * 3));

    raw.extend_from_slice(&SPZ_MAGIC.to_le_bytes());
    raw.extend_from_slice(&SPZ_VERSION.to_le_bytes());
    raw.extend_from_slice(&(n as u32).to_le_bytes());
    raw.push(sh_degree as u8);
    raw.push(SPZ_FRACTIONAL_BITS);
    raw.push(0); // flags: not antialiased
    raw.push(0); // reserved

    // Positions: 24-bit little-endian signed fixed point.
    let scale = (1i32 << SPZ_FRACTIONAL_BITS) as f32;
    for i in 0..n {
        for k2 in 0..3 {
            let fixed = (cloud.positions[i][k2] * scale).round() as i32;
            raw.push((fixed & 0xff) as u8);
            raw.push(((fixed >> 8) & 0xff) as u8);
            raw.push(((fixed >> 16) & 0xff) as u8);
        }
    }
    // Alphas.
    for i in 0..n {
        raw.push(to_u8(cloud.opacity(i) * 255.0));
    }
    // Colours: the DC band, not the resolved RGB.
    for i in 0..n {
        for c in 0..3 {
            raw.push(to_u8(
                cloud.sh_dc[i][c] * (SPZ_COLOR_SCALE * 255.0) + 0.5 * 255.0,
            ));
        }
    }
    // Scales: log-scale offset by 10 and scaled by 16.
    for i in 0..n {
        for c in 0..3 {
            raw.push(to_u8((cloud.scales_log[i][c] + 10.0) * 16.0));
        }
    }
    // Rotations: xyz only, with the sign fixed so w >= 0.
    for i in 0..n {
        let q = cloud.unit_rot(i);
        let s = if q[0] < 0.0 { -1.0 } else { 1.0 };
        for c in 1..4 {
            raw.push(to_u8(q[c] * s * 127.5 + 127.5));
        }
    }
    // Higher-order SH, coefficient-major, with coarser buckets above band 1.
    if k > 0 {
        for i in 0..n {
            let base = i * k * 3;
            for j in 0..k {
                for c in 0..3 {
                    // PLY stores channel-major; SPZ wants coefficient-major.
                    let v = cloud.sh_rest[base + c * k + j];
                    let bucket = if j < 3 { 8 } else { 16 };
                    raw.push(quantize_sh(v, bucket));
                }
            }
        }
    }

    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(&raw).map_err(|e| e.to_string())?;
    enc.finish().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud(n: usize, rest_per_channel: usize) -> SplatCloud {
        let mut c = SplatCloud {
            rest_per_channel,
            ..Default::default()
        };
        for i in 0..n {
            let f = i as f32;
            c.positions.push([f * 0.5, -f, 2.0 + f]);
            c.scales_log.push([-2.0, -2.5, -3.0]);
            // A rotation with negative w to exercise the SPZ sign fix.
            c.rot_wxyz.push([-0.7071068, 0.7071068, 0.0, 0.0]);
            c.opacity_logit.push(0.8);
            c.sh_dc.push([0.4, -0.1, 0.7]);
            for j in 0..rest_per_channel * 3 {
                c.sh_rest.push(0.05 * j as f32 - 0.2);
            }
        }
        c
    }

    #[test]
    fn format_parsing_and_extensions() {
        assert_eq!(Format::parse("ply"), Some(Format::Ply));
        assert_eq!(Format::parse(".SPLAT"), Some(Format::Splat));
        assert_eq!(Format::parse("spz"), Some(Format::Spz));
        assert_eq!(Format::parse("obj"), None);
        assert_eq!(Format::from_path(Path::new("a/b.spz")), Format::Spz);
        // Unknown extensions fall back to the default format.
        assert_eq!(Format::from_path(Path::new("a/b.xyz")), Format::Ply);
    }

    #[test]
    fn splat_encoding_has_the_expected_stride_and_fields() {
        let c = cloud(3, 0);
        let b = encode_splat(&c);
        assert_eq!(b.len(), 3 * 32);

        // Splat 1: position round-trips exactly as f32.
        let px = f32::from_le_bytes(b[32..36].try_into().unwrap());
        assert!((px - 0.5).abs() < 1e-7);
        // Scale is stored linear, not log.
        let sx = f32::from_le_bytes(b[32 + 12..32 + 16].try_into().unwrap());
        assert!((sx - (-2.0f32).exp()).abs() < 1e-6, "{sx}");
        // Colour byte matches the DC transform.
        let r = b[32 + 24];
        assert_eq!(r, to_u8((0.5 + SH_C0 * 0.4) * 255.0));
        // Alpha byte is the sigmoid of the logit.
        let a = b[32 + 27];
        let expect = 1.0 / (1.0 + (-0.8f32).exp());
        assert_eq!(a, to_u8(expect * 255.0));
        // Rotation is (w, x, y, z) mapped through q*128+128.
        let q = c.unit_rot(1);
        assert_eq!(b[32 + 28], to_u8(q[0] * 128.0 + 128.0));
        assert_eq!(b[32 + 31], to_u8(q[3] * 128.0 + 128.0));
    }

    /// Decode an SPZ stream back into the quantized fields, mirroring the
    /// reference reader, so the encoder is checked against a real layout
    /// rather than against itself.
    fn decode_spz(bytes: &[u8]) -> (u32, u8, u8, Vec<[f32; 3]>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
        use flate2::read::GzDecoder;
        use std::io::Read;
        let mut raw = Vec::new();
        GzDecoder::new(bytes).read_to_end(&mut raw).unwrap();

        assert_eq!(u32::from_le_bytes(raw[0..4].try_into().unwrap()), SPZ_MAGIC);
        assert_eq!(u32::from_le_bytes(raw[4..8].try_into().unwrap()), SPZ_VERSION);
        let n = u32::from_le_bytes(raw[8..12].try_into().unwrap());
        let sh_degree = raw[12];
        let frac = raw[13];
        assert_eq!(raw[14], 0);
        assert_eq!(raw[15], 0);

        let np = n as usize;
        let mut at = 16usize;
        let mut positions = Vec::with_capacity(np);
        let inv = 1.0f32 / (1i32 << frac) as f32;
        for _ in 0..np {
            let mut p = [0.0f32; 3];
            for v in p.iter_mut() {
                let mut fixed =
                    (raw[at] as i32) | ((raw[at + 1] as i32) << 8) | ((raw[at + 2] as i32) << 16);
                if fixed & 0x0080_0000 != 0 {
                    fixed |= -0x0100_0000; // sign-extend 24 bits
                }
                *v = fixed as f32 * inv;
                at += 3;
            }
            positions.push(p);
        }
        let take = |at: &mut usize, count: usize| {
            let s = raw[*at..*at + count].to_vec();
            *at += count;
            s
        };
        let alphas = take(&mut at, np);
        let colors = take(&mut at, np * 3);
        let scales = take(&mut at, np * 3);
        let rots = take(&mut at, np * 3);
        let sh_dim = match sh_degree {
            0 => 0,
            1 => 3,
            2 => 8,
            _ => 15,
        };
        let sh = take(&mut at, np * sh_dim * 3);
        assert_eq!(at, raw.len(), "trailing bytes in SPZ payload");
        (n, sh_degree, frac, positions, alphas, colors, scales, rots, sh)
    }

    #[test]
    fn spz_header_and_payload_match_the_container_layout() {
        let c = cloud(4, 0);
        let bytes = encode_spz(&c).unwrap();
        // gzip magic
        assert_eq!(&bytes[0..2], &[0x1f, 0x8b]);

        let (n, deg, frac, pos, alphas, colors, scales, rots, sh) = decode_spz(&bytes);
        assert_eq!(n, 4);
        assert_eq!(deg, 0);
        assert_eq!(frac, SPZ_FRACTIONAL_BITS);
        assert!(sh.is_empty());
        assert_eq!(alphas.len(), 4);
        assert_eq!(colors.len(), 12);
        assert_eq!(scales.len(), 12);
        assert_eq!(rots.len(), 12);

        // Positions survive the fixed-point round trip within one LSB, and
        // the negative coordinate proves the sign extension is right.
        for i in 0..4 {
            for k in 0..3 {
                let err = (pos[i][k] - c.positions[i][k]).abs();
                assert!(err < 1.0 / 4096.0, "{i},{k}: {} vs {}", pos[i][k], c.positions[i][k]);
            }
        }
        assert!(pos[1][1] < 0.0, "negative coordinate must decode negative");
    }

    #[test]
    fn spz_rotation_is_stored_xyz_with_a_non_negative_w() {
        let c = cloud(1, 0);
        let bytes = encode_spz(&c).unwrap();
        let (_, _, _, _, _, _, _, rots, _) = decode_spz(&bytes);

        // Input quaternion has w < 0, so the encoder must flip the sign.
        let q = c.unit_rot(0);
        assert!(q[0] < 0.0);
        let x = (rots[0] as f32 - 127.5) / 127.5;
        assert!((x - (-q[1])).abs() < 0.01, "{x} vs {}", -q[1]);

        // Recovering w from the stored xyz gives a unit quaternion.
        let y = (rots[1] as f32 - 127.5) / 127.5;
        let z = (rots[2] as f32 - 127.5) / 127.5;
        let w2 = 1.0 - (x * x + y * y + z * z);
        assert!(w2 > -0.01, "xyz must lie inside the unit ball: {w2}");
    }

    #[test]
    fn spz_scales_and_colors_use_the_documented_transforms() {
        let c = cloud(1, 0);
        let bytes = encode_spz(&c).unwrap();
        let (_, _, _, _, alphas, colors, scales, _, _) = decode_spz(&bytes);

        assert_eq!(scales[0], to_u8((-2.0 + 10.0) * 16.0));
        assert_eq!(colors[0], to_u8(0.4 * (SPZ_COLOR_SCALE * 255.0) + 127.5));
        assert_eq!(alphas[0], to_u8(c.opacity(0) * 255.0));
    }

    #[test]
    fn spz_sh_is_reordered_to_coefficient_major_with_bucketed_quantization() {
        let c = cloud(1, 3); // degree 1: 3 coefficients per channel
        let bytes = encode_spz(&c).unwrap();
        let (_, deg, _, _, _, _, _, _, sh) = decode_spz(&bytes);
        assert_eq!(deg, 1);
        assert_eq!(sh.len(), 9);

        // PLY channel-major -> SPZ coefficient-major: sh[j*3 + ch] == rest[ch*3 + j]
        for j in 0..3usize {
            for ch in 0..3usize {
                let want = quantize_sh(c.sh_rest[ch * 3 + j], 8);
                assert_eq!(sh[j * 3 + ch], want, "coeff {j} channel {ch}");
            }
        }
        // Degree-1 coefficients use the 8-wide bucket grid.
        assert!(sh.iter().all(|v| *v as i32 % 8 == 0 || *v == 255));
    }

    #[test]
    fn spz_degree_three_uses_the_coarse_bucket_above_band_one() {
        let c = cloud(1, 15);
        let bytes = encode_spz(&c).unwrap();
        let (_, deg, _, _, _, _, _, _, sh) = decode_spz(&bytes);
        assert_eq!(deg, 3);
        assert_eq!(sh.len(), 45);
        // Coefficients 3.. are bucketed to a multiple of 16.
        for j in 3..15usize {
            for ch in 0..3usize {
                let want = quantize_sh(c.sh_rest[ch * 15 + j], 16);
                assert_eq!(sh[j * 3 + ch], want);
            }
        }
    }

    #[test]
    fn empty_clouds_encode_without_panicking() {
        let c = SplatCloud::default();
        assert!(encode_splat(&c).is_empty());
        let bytes = encode_spz(&c).unwrap();
        let (n, _, _, _, _, _, _, _, _) = decode_spz(&bytes);
        assert_eq!(n, 0);
    }

    #[test]
    fn writing_each_format_produces_a_readable_file() {
        let dir = std::env::temp_dir().join("instasplatter_export_test");
        std::fs::create_dir_all(&dir).unwrap();
        let c = cloud(5, 3);
        for f in [Format::Ply, Format::Splat, Format::Spz] {
            let p = dir.join(format!("scene.{}", f.extension()));
            write(&p, &c, f).unwrap();
            let meta = std::fs::metadata(&p).unwrap();
            assert!(meta.len() > 0, "{f:?} produced an empty file");
        }
        // The PLY must load back through our own reader.
        let back = ply::read_ply(&dir.join("scene.ply")).unwrap();
        assert_eq!(back.len(), 5);
        assert_eq!(back.rest_per_channel, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
