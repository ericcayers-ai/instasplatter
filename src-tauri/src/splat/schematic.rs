//! Experimental Minecraft schematic export (Sponge Schematic v2 `.schem`).
//!
//! Voxelizes a Gaussian splat cloud into a WorldEdit-compatible region using
//! a vanilla concrete palette. Gated behind Experimental Mode in the UI/IPC.

use super::{SplatCloud, SH_C0};
use fastnbt::{ByteArray, IntArray, SerOpts};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Minecraft Java 1.21.4 data version — modern enough for concrete block names.
pub const DEFAULT_DATA_VERSION: i32 = 4189;

/// Longest schematic axis in blocks when the caller does not override.
pub const DEFAULT_MAX_EXTENT: u32 = 128;

const MIN_EXTENT: u32 = 16;
const MAX_EXTENT: u32 = 256;

/// Export knobs for schematic voxelization.
#[derive(Debug, Clone)]
pub struct SchematicOptions {
    /// Longest axis of the output region, in blocks (clamped 16..=256).
    pub max_extent: u32,
    /// Ignore Gaussians below this opacity (0..1).
    pub opacity_min: f32,
    /// Robust-bounds quantile used to reject floaters (0..1).
    pub bounds_quantile: f32,
    /// Minecraft DataVersion stamped into the schematic.
    pub data_version: i32,
    /// Schematic metadata name.
    pub name: String,
    /// Schematic metadata author.
    pub author: String,
}

impl Default for SchematicOptions {
    fn default() -> Self {
        Self {
            max_extent: DEFAULT_MAX_EXTENT,
            opacity_min: 0.1,
            bounds_quantile: 0.95,
            data_version: DEFAULT_DATA_VERSION,
            name: "InstaSplatter".into(),
            author: "InstaSplatter".into(),
        }
    }
}

impl SchematicOptions {
    pub fn clamp(mut self) -> Self {
        self.max_extent = self.max_extent.clamp(MIN_EXTENT, MAX_EXTENT);
        self.opacity_min = self.opacity_min.clamp(0.0, 1.0);
        self.bounds_quantile = self.bounds_quantile.clamp(0.5, 1.0);
        self
    }
}

/// Result of voxelizing a cloud before NBT write.
#[derive(Debug, Clone)]
pub struct VoxelGrid {
    pub width: u32,
    pub height: u32,
    pub length: u32,
    /// Palette index per cell, Y-up Minecraft order: `x + z*W + y*W*L`.
    pub indices: Vec<u16>,
    /// Block state strings; index 0 is always `minecraft:air`.
    pub palette: Vec<String>,
    pub occupied: u32,
    pub metres_per_block: f64,
}

/// Vanilla concrete colours used for photogrammetry-style builds.
struct ConcreteSwatch {
    name: &'static str,
    rgb: [f32; 3],
}

const CONCRETE: &[ConcreteSwatch] = &[
    ConcreteSwatch {
        name: "minecraft:white_concrete",
        rgb: [207.0, 213.0, 214.0],
    },
    ConcreteSwatch {
        name: "minecraft:orange_concrete",
        rgb: [224.0, 97.0, 0.0],
    },
    ConcreteSwatch {
        name: "minecraft:magenta_concrete",
        rgb: [169.0, 48.0, 159.0],
    },
    ConcreteSwatch {
        name: "minecraft:light_blue_concrete",
        rgb: [36.0, 137.0, 199.0],
    },
    ConcreteSwatch {
        name: "minecraft:yellow_concrete",
        rgb: [241.0, 175.0, 21.0],
    },
    ConcreteSwatch {
        name: "minecraft:lime_concrete",
        rgb: [94.0, 169.0, 24.0],
    },
    ConcreteSwatch {
        name: "minecraft:pink_concrete",
        rgb: [214.0, 101.0, 143.0],
    },
    ConcreteSwatch {
        name: "minecraft:gray_concrete",
        rgb: [55.0, 58.0, 62.0],
    },
    ConcreteSwatch {
        name: "minecraft:light_gray_concrete",
        rgb: [125.0, 125.0, 115.0],
    },
    ConcreteSwatch {
        name: "minecraft:cyan_concrete",
        rgb: [21.0, 119.0, 136.0],
    },
    ConcreteSwatch {
        name: "minecraft:purple_concrete",
        rgb: [100.0, 32.0, 156.0],
    },
    ConcreteSwatch {
        name: "minecraft:blue_concrete",
        rgb: [45.0, 47.0, 143.0],
    },
    ConcreteSwatch {
        name: "minecraft:brown_concrete",
        rgb: [96.0, 60.0, 32.0],
    },
    ConcreteSwatch {
        name: "minecraft:green_concrete",
        rgb: [73.0, 91.0, 36.0],
    },
    ConcreteSwatch {
        name: "minecraft:red_concrete",
        rgb: [142.0, 33.0, 33.0],
    },
    ConcreteSwatch {
        name: "minecraft:black_concrete",
        rgb: [8.0, 10.0, 15.0],
    },
];

/// Encode a non-negative integer as a Minecraft/Sponge varint into `out`.
pub fn encode_varint(mut value: u32, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Decode one Minecraft varint; returns `(value, bytes_consumed)`.
pub fn decode_varint(bytes: &[u8]) -> Result<(u32, usize), String> {
    let mut value: u32 = 0;
    let mut shift = 0u32;
    for (i, &b) in bytes.iter().enumerate() {
        if i >= 5 {
            return Err("varint too long".into());
        }
        value |= u32::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok((value, i + 1));
        }
        shift += 7;
    }
    Err("truncated varint".into())
}

/// Nearest concrete block for an sRGB triple in 0..255.
pub fn nearest_concrete(rgb: [f32; 3]) -> &'static str {
    let mut best = CONCRETE[0].name;
    let mut best_d = f32::INFINITY;
    for sw in CONCRETE {
        let dr = rgb[0] - sw.rgb[0];
        let dg = rgb[1] - sw.rgb[1];
        let db = rgb[2] - sw.rgb[2];
        let d = dr * dr + dg * dg + db * db;
        if d < best_d {
            best_d = d;
            best = sw.name;
        }
    }
    best
}

fn splat_rgb(cloud: &SplatCloud, i: usize) -> [f32; 3] {
    let c = cloud.sh_dc[i];
    [
        ((0.5 + SH_C0 * c[0]) * 255.0).clamp(0.0, 255.0),
        ((0.5 + SH_C0 * c[1]) * 255.0).clamp(0.0, 255.0),
        ((0.5 + SH_C0 * c[2]) * 255.0).clamp(0.0, 255.0),
    ]
}

/// Build an occupancy grid from opaque Gaussians.
pub fn voxelize_cloud(cloud: &SplatCloud, opts: &SchematicOptions) -> Result<VoxelGrid, String> {
    let opts = opts.clone().clamp();
    if cloud.is_empty() {
        return Err("Cannot export an empty splat cloud as a schematic.".into());
    }

    let (centroid, radius) = cloud.robust_bounds(opts.bounds_quantile);
    if radius < 1e-6 {
        return Err("Splat cloud has zero extent; nothing to voxelize.".into());
    }

    // Fit the robust diameter into max_extent blocks.
    let diameter = (2.0 * radius as f64).max(1e-3);
    let metres_per_block = diameter / opts.max_extent as f64;

    // Tight AABB over opaque points that fall inside the robust ball.
    let r2 = (radius as f64 * 1.05).powi(2);
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut kept = 0usize;
    for i in 0..cloud.len() {
        if cloud.opacity(i) < opts.opacity_min {
            continue;
        }
        let p = cloud.positions[i];
        let dx = p[0] as f64 - centroid[0] as f64;
        let dy = p[1] as f64 - centroid[1] as f64;
        let dz = p[2] as f64 - centroid[2] as f64;
        if dx * dx + dy * dy + dz * dz > r2 {
            continue;
        }
        kept += 1;
        for k in 0..3 {
            let v = p[k] as f64;
            min[k] = min[k].min(v);
            max[k] = max[k].max(v);
        }
    }
    if kept == 0 {
        return Err(
            "No Gaussians passed the opacity filter; lower opacityMin or check the splat.".into(),
        );
    }

    // Pad half a block so surface samples are not clipped.
    let pad = metres_per_block * 0.5;
    for k in 0..3 {
        min[k] -= pad;
        max[k] += pad;
    }

    let dim = |k: usize| -> u32 {
        let span = (max[k] - min[k]).max(metres_per_block);
        ((span / metres_per_block).ceil() as u32).clamp(1, MAX_EXTENT)
    };
    let width = dim(0);
    let height = dim(1);
    let length = dim(2);
    if width as u64 * height as u64 * length as u64 > (MAX_EXTENT as u64).pow(3) {
        return Err("Schematic dimensions exceed the experimental safety cap.".into());
    }

    let n = (width as usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(length as usize))
        .ok_or_else(|| "Schematic volume overflow".to_string())?;

    let mut sum_r = vec![0.0f32; n];
    let mut sum_g = vec![0.0f32; n];
    let mut sum_b = vec![0.0f32; n];
    let mut sum_w = vec![0.0f32; n];

    let idx = |x: u32, y: u32, z: u32| -> usize {
        (x + z * width + y * width * length) as usize
    };

    for i in 0..cloud.len() {
        let opacity = cloud.opacity(i);
        if opacity < opts.opacity_min {
            continue;
        }
        let p = cloud.positions[i];
        let dx = p[0] as f64 - centroid[0] as f64;
        let dy = p[1] as f64 - centroid[1] as f64;
        let dz = p[2] as f64 - centroid[2] as f64;
        if dx * dx + dy * dy + dz * dz > r2 {
            continue;
        }

        let fx = ((p[0] as f64 - min[0]) / metres_per_block).floor() as i64;
        let fy = ((p[1] as f64 - min[1]) / metres_per_block).floor() as i64;
        let fz = ((p[2] as f64 - min[2]) / metres_per_block).floor() as i64;

        // Stamp a small footprint from the Gaussian scale so thin surfaces fill.
        let scale = cloud.scale(i);
        let mean_s = ((scale[0] + scale[1] + scale[2]) / 3.0) as f64;
        let radius_blocks = ((mean_s / metres_per_block).ceil() as i64).clamp(0, 2);

        let rgb = splat_rgb(cloud, i);
        let w = opacity.max(0.05);

        for oy in -radius_blocks..=radius_blocks {
            for oz in -radius_blocks..=radius_blocks {
                for ox in -radius_blocks..=radius_blocks {
                    let x = fx + ox;
                    let y = fy + oy;
                    let z = fz + oz;
                    if x < 0 || y < 0 || z < 0 {
                        continue;
                    }
                    let x = x as u32;
                    let y = y as u32;
                    let z = z as u32;
                    if x >= width || y >= height || z >= length {
                        continue;
                    }
                    let falloff = 1.0 / (1.0 + (ox * ox + oy * oy + oz * oz) as f32);
                    let wi = w * falloff;
                    let j = idx(x, y, z);
                    sum_r[j] += rgb[0] * wi;
                    sum_g[j] += rgb[1] * wi;
                    sum_b[j] += rgb[2] * wi;
                    sum_w[j] += wi;
                }
            }
        }
    }

    // Collect used concrete names → compact palette (air = 0).
    let mut name_to_pal: BTreeMap<String, u16> = BTreeMap::new();
    name_to_pal.insert("minecraft:air".into(), 0);
    let mut palette = vec!["minecraft:air".to_string()];
    let mut indices = vec![0u16; n];
    let mut occupied = 0u32;

    for j in 0..n {
        if sum_w[j] < 1e-4 {
            continue;
        }
        let rgb = [
            sum_r[j] / sum_w[j],
            sum_g[j] / sum_w[j],
            sum_b[j] / sum_w[j],
        ];
        let block = nearest_concrete(rgb).to_string();
        let pal = if let Some(&p) = name_to_pal.get(&block) {
            p
        } else {
            let p = palette.len() as u16;
            name_to_pal.insert(block.clone(), p);
            palette.push(block);
            p
        };
        indices[j] = pal;
        occupied += 1;
    }

    if occupied == 0 {
        return Err("Voxelization produced only air.".into());
    }

    Ok(VoxelGrid {
        width,
        height,
        length,
        indices,
        palette,
        occupied,
        metres_per_block,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SchematicV2 {
    version: i32,
    data_version: i32,
    metadata: SchematicMeta,
    width: i16,
    height: i16,
    length: i16,
    offset: IntArray,
    palette_max: i32,
    palette: BTreeMap<String, i32>,
    block_data: ByteArray,
    block_entities: Vec<()>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SchematicMeta {
    name: String,
    author: String,
    date: i64,
}

fn encode_block_data(indices: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(indices.len());
    for &i in indices {
        encode_varint(u32::from(i), &mut out);
    }
    out
}

/// Write a Sponge Schematic v2 `.schem` (Gzip NBT, root name `Schematic`).
pub fn write_schem(path: &Path, grid: &VoxelGrid, opts: &SchematicOptions) -> Result<(), String> {
    let opts = opts.clone().clamp();
    if grid.width > u16::MAX as u32
        || grid.height > u16::MAX as u32
        || grid.length > u16::MAX as u32
    {
        return Err("Schematic dimensions do not fit unsigned short Width/Height/Length.".into());
    }

    let mut palette = BTreeMap::new();
    for (i, name) in grid.palette.iter().enumerate() {
        palette.insert(name.clone(), i as i32);
    }

    let date_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let schem = SchematicV2 {
        version: 2,
        data_version: opts.data_version,
        metadata: SchematicMeta {
            name: opts.name.clone(),
            author: opts.author.clone(),
            date: date_ms,
        },
        width: grid.width as i16,
        height: grid.height as i16,
        length: grid.length as i16,
        offset: IntArray::new(vec![0, 0, 0]),
        palette_max: grid.palette.len() as i32,
        palette,
        block_data: ByteArray::new(
            encode_block_data(&grid.indices)
                .into_iter()
                .map(|b| b as i8)
                .collect(),
        ),
        block_entities: Vec::new(),
    };

    let nbt = fastnbt::to_bytes_with_opts(&schem, SerOpts::new().root_name("Schematic"))
        .map_err(|e| format!("NBT encode failed: {e}"))?;

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
        }
    }

    let file = File::create(path).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    let mut enc = GzEncoder::new(file, Compression::default());
    enc.write_all(&nbt)
        .map_err(|e| format!("Gzip write failed: {e}"))?;
    enc.finish()
        .map_err(|e| format!("Gzip finish failed: {e}"))?;
    Ok(())
}

/// Convenience: voxelize then write.
pub fn export_schematic(
    cloud: &SplatCloud,
    path: &Path,
    opts: &SchematicOptions,
) -> Result<VoxelGrid, String> {
    let grid = voxelize_cloud(cloud, opts)?;
    write_schem(path, &grid, opts)?;
    Ok(grid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use std::io::Read;

    fn make_cloud(points: &[([f32; 3], [f32; 3], f32)]) -> SplatCloud {
        let mut cloud = SplatCloud::default();
        for &(pos, rgb, opacity) in points {
            cloud.positions.push(pos);
            cloud.scales_log.push([(-3.0f32).ln(); 3]);
            cloud.rot_wxyz.push([1.0, 0.0, 0.0, 0.0]);
            // logit(opacity)
            let o = opacity.clamp(1e-4, 1.0 - 1e-4);
            cloud.opacity_logit.push((o / (1.0 - o)).ln());
            cloud.sh_dc.push([
                (rgb[0] / 255.0 - 0.5) / SH_C0,
                (rgb[1] / 255.0 - 0.5) / SH_C0,
                (rgb[2] / 255.0 - 0.5) / SH_C0,
            ]);
        }
        cloud
    }

    #[test]
    fn varint_round_trips() {
        for v in [0u32, 1, 127, 128, 255, 300, 16_383, 16_384, 1_000_000] {
            let mut buf = Vec::new();
            encode_varint(v, &mut buf);
            let (got, n) = decode_varint(&buf).unwrap();
            assert_eq!(got, v);
            assert_eq!(n, buf.len());
        }
    }

    #[test]
    fn nearest_concrete_picks_obvious_colours() {
        assert_eq!(nearest_concrete([250.0, 250.0, 250.0]), "minecraft:white_concrete");
        assert_eq!(nearest_concrete([10.0, 10.0, 10.0]), "minecraft:black_concrete");
        assert_eq!(nearest_concrete([200.0, 40.0, 40.0]), "minecraft:red_concrete");
        assert_eq!(nearest_concrete([40.0, 50.0, 160.0]), "minecraft:blue_concrete");
    }

    #[test]
    fn empty_cloud_errors() {
        let err = voxelize_cloud(&SplatCloud::default(), &SchematicOptions::default()).unwrap_err();
        assert!(err.to_lowercase().contains("empty"));
    }

    #[test]
    fn voxelize_red_cluster_uses_red_concrete() {
        let cloud = make_cloud(&[
            ([0.0, 0.0, 0.0], [200.0, 30.0, 30.0], 0.9),
            ([0.05, 0.0, 0.0], [190.0, 25.0, 25.0], 0.9),
            ([0.0, 0.05, 0.0], [210.0, 35.0, 35.0], 0.85),
            ([0.0, 0.0, 0.05], [180.0, 20.0, 20.0], 0.8),
        ]);
        let mut opts = SchematicOptions::default();
        opts.max_extent = 32;
        let grid = voxelize_cloud(&cloud, &opts).unwrap();
        assert!(grid.occupied >= 1);
        assert!(grid.palette.iter().any(|p| p == "minecraft:red_concrete"));
        assert_eq!(grid.palette[0], "minecraft:air");
        assert!(grid.width >= 1 && grid.height >= 1 && grid.length >= 1);
    }

    #[test]
    fn write_schem_is_valid_gzip_nbt() {
        let cloud = make_cloud(&[
            ([0.0, 0.0, 0.0], [20.0, 120.0, 40.0], 0.95),
            ([0.1, 0.0, 0.0], [30.0, 130.0, 50.0], 0.9),
            ([0.0, 0.1, 0.0], [25.0, 110.0, 45.0], 0.9),
            ([0.0, 0.0, 0.1], [35.0, 140.0, 55.0], 0.85),
            ([0.05, 0.05, 0.05], [40.0, 100.0, 40.0], 0.8),
        ]);
        let dir = std::env::temp_dir().join("instasplatter_schem_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scene.schem");

        let mut opts = SchematicOptions::default();
        opts.max_extent = 32;
        opts.name = "TestScene".into();
        let grid = export_schematic(&cloud, &path, &opts).unwrap();
        assert!(grid.occupied > 0);
        assert!(path.is_file());

        let compressed = std::fs::read(&path).unwrap();
        assert!(compressed.len() > 20);
        // Gzip magic
        assert_eq!(&compressed[0..2], &[0x1f, 0x8b]);

        let mut dec = GzDecoder::new(&compressed[..]);
        let mut nbt = Vec::new();
        dec.read_to_end(&mut nbt).unwrap();
        // Unnamed or named compound: tag type 10 at start of named NBT
        assert_eq!(nbt[0], 10); // TAG_Compound
                                // Root name "Schematic"
        assert!(nbt.windows(9).any(|w| w == b"Schematic"));
        assert!(nbt.windows(7).any(|w| w == b"Version"));
        assert!(nbt.windows(9).any(|w| w == b"BlockData"));
        assert!(nbt.windows(7).any(|w| w == b"Palette"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn options_clamp_extent() {
        let o = SchematicOptions {
            max_extent: 8,
            ..Default::default()
        }
        .clamp();
        assert_eq!(o.max_extent, 16);
        let o = SchematicOptions {
            max_extent: 9999,
            ..Default::default()
        }
        .clamp();
        assert_eq!(o.max_extent, 256);
    }
}
