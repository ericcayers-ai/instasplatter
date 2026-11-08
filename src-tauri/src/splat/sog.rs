//! SOG v2 reader — the container modern splat delivery ships as.
//!
//! A `.sog` file is a ZIP bundle written by `@playcanvas/splat-transform` (MIT,
//! the export tool the RealityLog side already runs): `meta.json` plus
//! lossless-WebP attribute images that hold one texel per splat. That is what
//! makes a 100M-splat scene deliverable — the attributes are quantized into
//! image codecs instead of a float PLY — and it is the read path the host was
//! missing (see `docs/AUDIT-2026-11-03.md` §8).
//!
//! ## Layout (measured on real `splat-transform v3.4.2` archives)
//!
//! | entry | channels | meaning |
//! |---|---|---|
//! | `meta.json` | — | version, `count`, bounds, codebooks, file names |
//! | `means_l.webp` | RGB | low byte of each axis' 16-bit position |
//! | `means_u.webp` | RGB | high byte of the same position |
//! | `scales.webp` | RGB | index into `scales.codebook` → `ln(scale)` |
//! | `quats.webp` | RGBA | `byte/255*2-1` per component, channel order **not established** — see below |
//! | `sh0.webp` | RGBA | RGB index into `sh0.codebook` → DC term, A → opacity |
//! | `shN_*.webp` | — | higher-order SH palette (not carried, see below) |
//!
//! Splat `i` is the texel at `(i % width, i / width)` of every image, and the
//! images are padded out to a rectangle, so `count <= width * height`.
//!
//! ## What is measured, and what is not
//!
//! The reader was checked against the source PLY of a real archive
//! (`splat-transform v3.4.2`, 113 085 splats): every one of the 113 085 decoded
//! positions lands on a source-PLY splat within 0.05, opacity round-trips to
//! within 0.02 logit except where the 8-bit channel saturates, and scales/DC
//! resolve to the nearest codebook entry. Positions need the signed-logarithmic
//! decode above — read linearly, the same archive decodes to a ±4.5 box instead
//! of the source's −89.9 … 81.4.
//!
//! **Rotations are not reproduced.** The four quaternion channels do not form a
//! unit quaternion under any channel permutation (measured mean |dot| with the
//! source splat 0.5393, best permutation still 0.98 worst-case unit error), so
//! [`SplatCloud::rot_wxyz`] here is the archive's channel order as stored —
//! orientation is an open item, recorded in
//! `docs/evidence/sog-reader-2026-11-07.md`.
//!
//! Higher-order SH (`shN`) is a palette indirection the host has no consumer
//! for — [`SplatCloud::sh_rest`] stays empty and [`SogCloud::sh_bands_in_archive`]
//! reports what was left behind, so a caller can tell "DC-only cloud" from
//! "archive had no higher order".

use super::SplatCloud;
use serde::Deserialize;
use std::io::{Cursor, Read};
use std::path::Path;

/// The only container version `splat-transform` has written since 2025; the
/// attribute semantics below are version-2 semantics.
pub const SOG_VERSION: u32 = 2;

/// A reader result: the cloud plus what the archive carried but the host's
/// data model cannot hold.
#[derive(Debug, Clone, Default)]
pub struct SogCloud {
    pub cloud: SplatCloud,
    /// `shN.bands` from the archive (0 when the archive is DC-only).
    pub sh_bands_in_archive: u32,
}

/// Read a bundled `.sog` archive from disk.
pub fn read_sog(path: &Path) -> Result<SogCloud, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    read_sog_bytes(&bytes)
}

/// Read a bundled `.sog` archive held in memory.
pub fn read_sog_bytes(bytes: &[u8]) -> Result<SogCloud, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("A SOG file is a ZIP bundle, and this one is unreadable: {e}"))?;

    let meta: Meta = serde_json::from_slice(&archive_entry(&mut archive, "meta.json")?)
        .map_err(|e| format!("SOG meta.json is malformed: {e}"))?;
    if meta.version != SOG_VERSION {
        return Err(format!(
            "SOG meta.json declares version {}; this reader implements version {SOG_VERSION}",
            meta.version
        ));
    }

    let (means_l_name, means_u_name) = match meta.means.files.as_slice() {
        [l, u] => (l.clone(), u.clone()),
        other => {
            return Err(format!(
                "SOG means.files must name exactly the low and high byte images, found {other:?}"
            ))
        }
    };
    let means_l = AttributeImage::read(&mut archive, &means_l_name)?;
    let means_u = AttributeImage::read(&mut archive, &means_u_name)?;
    let scales = AttributeImage::read(&mut archive, single_file(&meta.scales.files, "scales")?)?;
    let quats = AttributeImage::read(&mut archive, single_file(&meta.quats.files, "quats")?)?;
    let sh0 = AttributeImage::read(&mut archive, single_file(&meta.sh0.files, "sh0")?)?;

    let count = meta.count;
    for (name, image) in [
        ("means_l", &means_l),
        ("means_u", &means_u),
        ("scales", &scales),
        ("quats", &quats),
        ("sh0", &sh0),
    ] {
        if image.texels() < count {
            return Err(format!(
                "SOG {name} image is {}x{} ({} texels) but meta.json declares {count} splats",
                image.width,
                image.height,
                image.texels()
            ));
        }
    }

    let mut cloud = SplatCloud {
        positions: Vec::with_capacity(count),
        scales_log: Vec::with_capacity(count),
        rot_wxyz: Vec::with_capacity(count),
        opacity_logit: Vec::with_capacity(count),
        sh_dc: Vec::with_capacity(count),
        sh_rest: Vec::new(),
        rest_per_channel: 0,
    };

    for i in 0..count {
        let lo = means_l.texel(i);
        let hi = means_u.texel(i);
        let mut position = [0.0f32; 3];
        for axis in 0..3 {
            let quantized = u16::from(hi[axis]) << 8 | u16::from(lo[axis]);
            let t = f32::from(quantized) / 65535.0;
            let t = meta.means.mins[axis] + t * (meta.means.maxs[axis] - meta.means.mins[axis]);
            // Positions are stored signed-logarithmically: fine resolution near
            // the origin, coarse far away, which is why a 9-unit box and a
            // 170-unit box share the same 16-bit texture budget.
            position[axis] = t.signum() * t.abs().exp_m1();
        }
        cloud.positions.push(position);

        let scale = scales.texel(i);
        let mut scales_log = [0.0f32; 3];
        for axis in 0..3 {
            scales_log[axis] = codebook_value(&meta.scales.codebook, scale[axis], "scales")?;
        }
        cloud.scales_log.push(scales_log);

        let q = quats.texel(i);
        cloud
            .rot_wxyz
            .push(std::array::from_fn(|k| f32::from(q[k]) / 255.0 * 2.0 - 1.0));

        let color = sh0.texel(i);
        cloud.sh_dc.push([
            codebook_value(&meta.sh0.codebook, color[0], "sh0")?,
            codebook_value(&meta.sh0.codebook, color[1], "sh0")?,
            codebook_value(&meta.sh0.codebook, color[2], "sh0")?,
        ]);
        cloud.opacity_logit.push(opacity_logit(color[3]));
    }

    Ok(SogCloud {
        cloud,
        sh_bands_in_archive: meta.sh_n.as_ref().map_or(0, |sh| sh.bands),
    })
}

/// The opacity byte is `sigmoid(opacity)` on a 0..255 scale; 0 and 255 are the
/// saturated ends of that scale, which the logit cannot represent, so they are
/// clamped rather than returned as infinities.
fn opacity_logit(byte: u8) -> f32 {
    match byte {
        0 => -20.0,
        255 => 20.0,
        b => {
            let o = f32::from(b) / 255.0;
            (o / (1.0 - o)).ln()
        }
    }
}

fn codebook_value(codebook: &[f32], label: u8, section: &str) -> Result<f32, String> {
    codebook.get(usize::from(label)).copied().ok_or_else(|| {
        format!(
            "SOG {section} image references codebook slot {label}, but the codebook has {} entries",
            codebook.len()
        )
    })
}

fn single_file<'a>(files: &'a [String], section: &str) -> Result<&'a str, String> {
    match files {
        [only] => Ok(only),
        other => Err(format!(
            "SOG {section}.files must name exactly one image, found {other:?}"
        )),
    }
}

/// Per-entry cap on a decompressed SOG member. A real bundle carries one small
/// `meta.json` plus a handful of lossless-WebP attribute images (a 113k-splat
/// archive is 4.2 MB in total), so 8 MiB per entry is far above anything genuine.
const MAX_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
/// Cap on decoded attribute-image texels (16384 x 1024, or 16.7 M splats). A SOG
/// texel is one splat, and the decode allocates four bytes per texel.
const MAX_TEXELS: usize = 16 * 1024 * 1024;

fn archive_entry(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<Vec<u8>, String> {
    let mut file = archive
        .by_name(name)
        .map_err(|_| format!("SOG bundle has no {name} entry"))?;
    // The ZIP central directory already carries the uncompressed length, so a
    // decompression bomb is rejected before a single byte is inflated. The
    // `take` below is the second line of defence for a lying header.
    let declared = file.size();
    if declared > MAX_ENTRY_BYTES {
        return Err(format!(
            "SOG entry {name} declares {declared} bytes, above the {MAX_ENTRY_BYTES}-byte per-entry limit"
        ));
    }
    let mut bytes = Vec::with_capacity(declared as usize);
    file.by_ref()
        .take(MAX_ENTRY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("SOG entry {name} is unreadable: {e}"))?;
    if bytes.len() as u64 > MAX_ENTRY_BYTES {
        return Err(format!(
            "SOG entry {name} inflates past the {MAX_ENTRY_BYTES}-byte per-entry limit"
        ));
    }
    Ok(bytes)
}

/// One decoded attribute image, RGBA8.
struct AttributeImage {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl AttributeImage {
    fn read(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Self, String> {
        let bytes = archive_entry(archive, name)?;
        let image = image::load_from_memory(&bytes)
            .map_err(|e| format!("SOG entry {name} is not a decodable WebP image: {e}"))?;
        let (width, height) = (image.width() as usize, image.height() as usize);
        // Decoded size follows the header, not the compressed size, so bound it
        // explicitly before `to_rgba8` allocates width * height * 4 bytes.
        if width.saturating_mul(height) > MAX_TEXELS {
            return Err(format!(
                "SOG entry {name} decodes to {width}x{height} ({} texels), above the {MAX_TEXELS}-texel limit",
                width.saturating_mul(height)
            ));
        }
        let rgba = image.to_rgba8();
        let (width, height) = rgba.dimensions();
        Ok(Self {
            width: width as usize,
            height: height as usize,
            pixels: rgba.into_raw(),
        })
    }

    fn texels(&self) -> usize {
        self.width.saturating_mul(self.height)
    }

    /// Splats are stored row-major, one texel each.
    fn texel(&self, i: usize) -> [u8; 4] {
        let o = i * 4;
        [
            self.pixels[o],
            self.pixels[o + 1],
            self.pixels[o + 2],
            self.pixels[o + 3],
        ]
    }
}

#[derive(Deserialize)]
struct Meta {
    version: u32,
    count: usize,
    means: MeansMeta,
    scales: CodebookMeta,
    quats: FilesMeta,
    sh0: CodebookMeta,
    #[serde(rename = "shN", default)]
    sh_n: Option<ShNMeta>,
}

#[derive(Deserialize)]
struct MeansMeta {
    mins: [f32; 3],
    maxs: [f32; 3],
    files: Vec<String>,
}

#[derive(Deserialize)]
struct CodebookMeta {
    codebook: Vec<f32>,
    files: Vec<String>,
}

#[derive(Deserialize)]
struct FilesMeta {
    files: Vec<String>,
}

#[derive(Deserialize)]
struct ShNMeta {
    bands: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::splat::ply;
    use std::collections::HashMap;

    /// The 8-bit opacity channel is `sigmoid(logit)`; these four pairs are the
    /// measured round trips from the real archive (`docs/evidence/
    /// sog-reader-2026-11-07.md`).
    #[test]
    fn the_opacity_channel_inverts_to_the_measured_logits() {
        for (byte, logit) in [
            (9u8, -3.308f32),
            (4u8, -4.139),
            (17u8, -2.639),
            (11u8, -3.099),
        ] {
            let got = opacity_logit(byte);
            assert!(
                (got - logit).abs() < 0.01,
                "opacity byte {byte} decoded to {got}, expected {logit}"
            );
        }
        assert_eq!(opacity_logit(0), -20.0);
        assert_eq!(opacity_logit(255), 20.0);
    }

    /// A `.sog` is a ZIP bundle; anything else fails and says why.
    #[test]
    fn refuses_anything_that_is_not_a_zip_bundle() {
        let err = read_sog_bytes(b"not a sog archive").expect_err("must be refused");
        assert!(err.contains("ZIP"), "{err}");
    }

    /// Counts SOG positions that land on a source-PLY position within `TOL`.
    fn count_position_matches(sog_positions: &[[f32; 3]], ply_positions: &[[f32; 3]]) -> usize {
        const CELL: f32 = 0.1;
        const TOL: f32 = 0.05;
        let cell = |v: f32| (v / CELL).round() as i32;
        let mut grid: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
        for (i, p) in ply_positions.iter().enumerate() {
            grid.entry((cell(p[0]), cell(p[1]), cell(p[2])))
                .or_default()
                .push(i);
        }
        let mut matched = 0usize;
        for p in sog_positions {
            let (cx, cy, cz) = (cell(p[0]), cell(p[1]), cell(p[2]));
            'search: for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if let Some(candidates) = grid.get(&(cx + dx, cy + dy, cz + dz)) {
                            for &j in candidates {
                                let q = ply_positions[j];
                                let d = (p[0] - q[0])
                                    .abs()
                                    .max((p[1] - q[1]).abs())
                                    .max((p[2] - q[2]).abs());
                                if d < TOL {
                                    matched += 1;
                                    break 'search;
                                }
                            }
                        }
                    }
                }
            }
        }
        matched
    }

    /// The end-to-end measurement, on the two fixtures that live outside this
    /// repo. Run once on a machine that has them:
    ///
    /// ```text
    /// SOG_ARCHIVE=<...>/p6.3-splat_7000-20260920T093656Z.sog \
    /// SOG_REFERENCE_PLY=<...>/gs_run/raw/brush_out/splat_7000.ply \
    /// cargo test --locked sog -- --ignored
    /// ```
    #[test]
    #[ignore = "needs SOG_ARCHIVE and SOG_REFERENCE_PLY; both fixtures live outside this repo"]
    fn reproduces_the_ply_it_was_exported_from() {
        let archive =
            std::env::var("SOG_ARCHIVE").expect("set SOG_ARCHIVE to a splat-transform .sog");
        let reference = std::env::var("SOG_REFERENCE_PLY")
            .expect("set SOG_REFERENCE_PLY to the PLY that archive was exported from");
        let doc = read_sog(Path::new(&archive)).expect("the archive must parse");
        let source = ply::read_ply(Path::new(&reference)).expect("the source PLY must parse");

        assert_eq!(doc.cloud.len(), 113_085, "decoded splat count");
        assert_eq!(source.len(), 113_085, "source splat count");
        assert_eq!(doc.sh_bands_in_archive, 3, "shN.bands in the archive");

        let matched = count_position_matches(&doc.cloud.positions, &source.positions);
        assert_eq!(matched, 113_085, "positions that land on a source splat");

        // Signed-logarithmic dequantization is what puts the decode on the
        // source PLY's box; read linearly it is a ±4.5 box instead.
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in &doc.cloud.positions {
            for (axis, value) in p.iter().enumerate() {
                lo[axis] = lo[axis].min(*value);
                hi[axis] = hi[axis].max(*value);
            }
        }
        assert!((lo[0] + 89.8959).abs() < 0.02, "x min {lo:?}");
        assert!((lo[1] + 81.1130).abs() < 0.02, "y min {lo:?}");
        assert!((lo[2] + 36.8797).abs() < 0.02, "z min {lo:?}");
        assert!((hi[0] - 81.4342).abs() < 0.02, "x max {hi:?}");
        assert!((hi[1] - 79.9316).abs() < 0.02, "y max {hi:?}");
        assert!((hi[2] - 75.9607).abs() < 0.02, "z max {hi:?}");

        // Opacity stays inside the sigmoid's range for every splat.
        for i in 0..doc.cloud.len() {
            let o = doc.cloud.opacity(i);
            assert!((0.0..1.0).contains(&o), "splat {i} opacity {o}");
        }

        // The quaternion channels are not a unit quaternion in this archive —
        // the reason rotations are reported as unverified rather than decoded.
        let mut norm_min = f32::MAX;
        let mut norm_max = f32::MIN;
        for q in &doc.cloud.rot_wxyz {
            let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
            norm_min = norm_min.min(n);
            norm_max = norm_max.max(n);
        }
        assert!(
            norm_min > 0.9 && norm_max < 1.6,
            "quat channel norms {norm_min}..{norm_max}"
        );
    }
}
