//! Splat export formats (ROADMAP-V2 1.6).
//!
//! PLY stays the default. `.splat` is the 32-byte-per-Gaussian layout the
//! common web viewers read, and `.spz` is Niantic's compressed format
//! (container version 4 — ZSTD parallel attribute streams).

use super::{ply, SplatCloud, SH_C0};
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

// ---- .spz (Niantic SPZ v4) -------------------------------------------------

const SPZ_MAGIC: u32 = 0x5053_474e; // "NGSP"
const SPZ_VERSION: u32 = 4;
const SPZ_FRACTIONAL_BITS: u8 = 12;
/// Niantic's fixed colour scale for the DC band.
const SPZ_COLOR_SCALE: f32 = 0.15;
const SPZ_NUM_STREAMS: u8 = 6;

/// Round `x * 128 + 128` onto a bucket grid, as SPZ does for SH coefficients.
fn quantize_sh(x: f32, bucket: i32) -> u8 {
    let q = (x * 128.0).round() as i32 + 128;
    let q = (q + bucket / 2) / bucket * bucket;
    q.clamp(0, 255) as u8
}

/// Pack a unit quaternion into SPZ v3/v4 smallest-three 32-bit encoding.
fn pack_quat_v4(q: [f32; 4]) -> [u8; 4] {
    // Find largest absolute component; flip so it is non-negative.
    let mut q = q;
    let mut largest = 0usize;
    let mut best = q[0].abs();
    for i in 1..4 {
        if q[i].abs() > best {
            best = q[i].abs();
            largest = i;
        }
    }
    if q[largest] < 0.0 {
        for v in &mut q {
            *v = -*v;
        }
    }
    let rest: [f32; 3] = match largest {
        0 => [q[1], q[2], q[3]],
        1 => [q[0], q[2], q[3]],
        2 => [q[0], q[1], q[3]],
        _ => [q[0], q[1], q[2]],
    };
    // 10-bit signed fixed: value ∈ [-1/√2, 1/√2] → [-511, 511]
    let scale = std::f32::consts::SQRT_2 * 511.0;
    let enc = |v: f32| -> u32 {
        let i = (v * scale).round().clamp(-511.0, 511.0) as i32;
        (i as u32) & 0x3ff
    };
    let packed: u32 = ((largest as u32) << 30)
        | (enc(rest[0]) << 20)
        | (enc(rest[1]) << 10)
        | enc(rest[2]);
    packed.to_le_bytes()
}

fn unpack_quat_v4(bytes: &[u8]) -> [f32; 4] {
    let packed = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let largest = (packed >> 30) as usize;
    let dec = |shift: u32| -> f32 {
        let mut bits = ((packed >> shift) & 0x3ff) as i32;
        if bits >= 512 {
            bits -= 1024; // sign-extend 10 bits
        }
        bits as f32 / (std::f32::consts::SQRT_2 * 511.0)
    };
    let a = dec(20);
    let b = dec(10);
    let c = dec(0);
    let mut q = [0.0f32; 4];
    match largest {
        0 => {
            q[1] = a;
            q[2] = b;
            q[3] = c;
        }
        1 => {
            q[0] = a;
            q[2] = b;
            q[3] = c;
        }
        2 => {
            q[0] = a;
            q[1] = b;
            q[3] = c;
        }
        _ => {
            q[0] = a;
            q[1] = b;
            q[2] = c;
        }
    }
    let sum = q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3];
    q[largest] = (1.0 - sum).max(0.0).sqrt();
    q
}

fn zstd_compress(raw: &[u8]) -> Result<Vec<u8>, String> {
    zstd::encode_all(raw, 3).map_err(|e| format!("SPZ zstd compress: {e}"))
}

/// Encode to official SPZ container version 4 (ZSTD parallel attribute streams).
pub fn encode_spz(cloud: &SplatCloud) -> Result<Vec<u8>, String> {
    let n = cloud.len();
    if n > u32::MAX as usize {
        return Err("Too many splats for the SPZ format.".into());
    }
    let sh_degree = cloud.sh_degree() as u8;
    let k = cloud.rest_per_channel;

    // ---- attribute streams (uncompressed) ----
    let mut positions = Vec::with_capacity(n * 9);
    let scale_fix = (1i32 << SPZ_FRACTIONAL_BITS) as f32;
    for i in 0..n {
        for c in 0..3 {
            let fixed = (cloud.positions[i][c] * scale_fix).round() as i32;
            positions.push((fixed & 0xff) as u8);
            positions.push(((fixed >> 8) & 0xff) as u8);
            positions.push(((fixed >> 16) & 0xff) as u8);
        }
    }

    let mut alphas = Vec::with_capacity(n);
    for i in 0..n {
        alphas.push(to_u8(cloud.opacity(i) * 255.0));
    }

    let mut colors = Vec::with_capacity(n * 3);
    for i in 0..n {
        for c in 0..3 {
            colors.push(to_u8(
                cloud.sh_dc[i][c] * (SPZ_COLOR_SCALE * 255.0) + 0.5 * 255.0,
            ));
        }
    }

    let mut scales = Vec::with_capacity(n * 3);
    for i in 0..n {
        for c in 0..3 {
            scales.push(to_u8((cloud.scales_log[i][c] + 10.0) * 16.0));
        }
    }

    let mut rotations = Vec::with_capacity(n * 4);
    for i in 0..n {
        rotations.extend_from_slice(&pack_quat_v4(cloud.unit_rot(i)));
    }

    let mut sh = Vec::new();
    if k > 0 {
        sh.reserve(n * k * 3);
        for i in 0..n {
            let base = i * k * 3;
            for j in 0..k {
                for c in 0..3 {
                    let v = cloud.sh_rest[base + c * k + j];
                    let bucket = if j < 3 { 8 } else { 16 };
                    sh.push(quantize_sh(v, bucket));
                }
            }
        }
    }

    let streams_raw = [positions, alphas, colors, scales, rotations, sh];
    let mut compressed: Vec<Vec<u8>> = Vec::with_capacity(6);
    let mut toc: Vec<(u64, u64)> = Vec::with_capacity(6);
    for raw in &streams_raw {
        let c = zstd_compress(raw)?;
        toc.push((c.len() as u64, raw.len() as u64));
        compressed.push(c);
    }

    let toc_byte_offset: u32 = 32; // no extensions
    let mut out: Vec<u8> = Vec::with_capacity(32 + 6 * 16 + compressed.iter().map(|c| c.len()).sum::<usize>());

    // 32-byte plaintext NgspFileHeader
    out.extend_from_slice(&SPZ_MAGIC.to_le_bytes());
    out.extend_from_slice(&SPZ_VERSION.to_le_bytes());
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.push(sh_degree);
    out.push(SPZ_FRACTIONAL_BITS);
    out.push(0); // flags
    out.push(SPZ_NUM_STREAMS);
    out.extend_from_slice(&toc_byte_offset.to_le_bytes());
    out.extend_from_slice(&[0u8; 12]); // reserved

    // TOC: N × [compressedSize u64, uncompressedSize u64]
    for (csize, usize_) in &toc {
        out.extend_from_slice(&csize.to_le_bytes());
        out.extend_from_slice(&usize_.to_le_bytes());
    }
    for c in &compressed {
        out.extend_from_slice(c);
    }
    Ok(out)
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

    /// Decode an SPZ v4 stream back into uncompressed attribute fields.
    fn decode_spz(
        bytes: &[u8],
    ) -> (
        u32,
        u8,
        u8,
        Vec<[f32; 3]>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
    ) {
        assert!(bytes.len() >= 32);
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), SPZ_MAGIC);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), SPZ_VERSION);
        let n = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let sh_degree = bytes[12];
        let frac = bytes[13];
        assert_eq!(bytes[14], 0); // flags
        let num_streams = bytes[15];
        assert_eq!(num_streams, SPZ_NUM_STREAMS);
        let toc_off = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        assert_eq!(toc_off, 32);

        let mut at = toc_off;
        let mut toc = Vec::new();
        for _ in 0..num_streams {
            let csize = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
            let usize_ = u64::from_le_bytes(bytes[at + 8..at + 16].try_into().unwrap());
            toc.push((csize as usize, usize_ as usize));
            at += 16;
        }
        let mut streams = Vec::new();
        for (csize, _) in &toc {
            let chunk = &bytes[at..at + *csize];
            let raw = zstd::decode_all(chunk).expect("zstd decode");
            streams.push(raw);
            at += *csize;
        }
        assert_eq!(at, bytes.len());

        let pos_raw = &streams[0];
        let alphas = streams[1].clone();
        let colors = streams[2].clone();
        let scales = streams[3].clone();
        let rots = streams[4].clone();
        let sh = streams[5].clone();

        let np = n as usize;
        let mut positions = Vec::with_capacity(np);
        let inv = 1.0f32 / (1i32 << frac) as f32;
        let mut p_at = 0usize;
        for _ in 0..np {
            let mut p = [0.0f32; 3];
            for v in p.iter_mut() {
                let mut fixed = (pos_raw[p_at] as i32)
                    | ((pos_raw[p_at + 1] as i32) << 8)
                    | ((pos_raw[p_at + 2] as i32) << 16);
                if fixed & 0x0080_0000 != 0 {
                    fixed |= -0x0100_0000;
                }
                *v = fixed as f32 * inv;
                p_at += 3;
            }
            positions.push(p);
        }
        (n, sh_degree, frac, positions, alphas, colors, scales, rots, sh)
    }

    #[test]
    fn spz_header_and_payload_match_the_container_layout() {
        let c = cloud(4, 0);
        let bytes = encode_spz(&c).unwrap();
        // SPZ v4 plaintext magic (not gzip).
        assert_eq!(&bytes[0..4], b"NGSP");

        let (n, deg, frac, pos, alphas, colors, scales, rots, sh) = decode_spz(&bytes);
        assert_eq!(n, 4);
        assert_eq!(deg, 0);
        assert_eq!(frac, SPZ_FRACTIONAL_BITS);
        assert!(sh.is_empty());
        assert_eq!(alphas.len(), 4);
        assert_eq!(colors.len(), 12);
        assert_eq!(scales.len(), 12);
        assert_eq!(rots.len(), 16); // 4 bytes per splat in v4

        for i in 0..4 {
            for k in 0..3 {
                let err = (pos[i][k] - c.positions[i][k]).abs();
                assert!(
                    err < 1.0 / 4096.0,
                    "{i},{k}: {} vs {}",
                    pos[i][k],
                    c.positions[i][k]
                );
            }
        }
        assert!(pos[1][1] < 0.0, "negative coordinate must decode negative");
    }

    #[test]
    fn spz_rotation_v4_round_trips_with_non_negative_largest() {
        let c = cloud(1, 0);
        let bytes = encode_spz(&c).unwrap();
        let (_, _, _, _, _, _, _, rots, _) = decode_spz(&bytes);

        let q_in = c.unit_rot(0);
        assert!(q_in[0] < 0.0);
        let q_out = unpack_quat_v4(&rots[0..4]);
        // Angular check: absolute dot of unit quaternions ≥ ~0.98 (sign may flip).
        let dot = (q_in[0] * q_out[0]
            + q_in[1] * q_out[1]
            + q_in[2] * q_out[2]
            + q_in[3] * q_out[3])
            .abs();
        assert!(dot > 0.98, "quat round-trip dot={dot}");
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

        for j in 0..3usize {
            for ch in 0..3usize {
                let want = quantize_sh(c.sh_rest[ch * 3 + j], 8);
                assert_eq!(sh[j * 3 + ch], want, "coeff {j} channel {ch}");
            }
        }
        assert!(sh.iter().all(|v| *v as i32 % 8 == 0 || *v == 255));
    }

    #[test]
    fn spz_degree_three_uses_the_coarse_bucket_above_band_one() {
        let c = cloud(1, 15);
        let bytes = encode_spz(&c).unwrap();
        let (_, deg, _, _, _, _, _, _, sh) = decode_spz(&bytes);
        assert_eq!(deg, 3);
        assert_eq!(sh.len(), 45);
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
