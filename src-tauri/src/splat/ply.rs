//! 3DGS PLY reading and writing.
//!
//! Reads the `binary_little_endian` layout produced by Brush, COLMAP and the
//! reference 3DGS implementations, and writes the same property order so the
//! result opens in third-party viewers unchanged.

use super::SplatCloud;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ScalarType {
    F32,
    F64,
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
}

impl ScalarType {
    fn parse(s: &str) -> Option<ScalarType> {
        Some(match s {
            "float" | "float32" => ScalarType::F32,
            "double" | "float64" => ScalarType::F64,
            "uchar" | "uint8" => ScalarType::U8,
            "char" | "int8" => ScalarType::I8,
            "ushort" | "uint16" => ScalarType::U16,
            "short" | "int16" => ScalarType::I16,
            "uint" | "uint32" => ScalarType::U32,
            "int" | "int32" => ScalarType::I32,
            _ => return None,
        })
    }

    fn size(self) -> usize {
        match self {
            ScalarType::U8 | ScalarType::I8 => 1,
            ScalarType::U16 | ScalarType::I16 => 2,
            ScalarType::F32 | ScalarType::U32 | ScalarType::I32 => 4,
            ScalarType::F64 => 8,
        }
    }

    fn read(self, b: &[u8]) -> f32 {
        match self {
            ScalarType::F32 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            ScalarType::F64 => f64::from_le_bytes([
                b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
            ]) as f32,
            ScalarType::U8 => b[0] as f32,
            ScalarType::I8 => (b[0] as i8) as f32,
            ScalarType::U16 => u16::from_le_bytes([b[0], b[1]]) as f32,
            ScalarType::I16 => i16::from_le_bytes([b[0], b[1]]) as f32,
            ScalarType::U32 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
            ScalarType::I32 => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32,
        }
    }
}

struct Prop {
    name: String,
    offset: usize,
    ty: ScalarType,
}

struct Header {
    vertex_count: usize,
    props: Vec<Prop>,
    stride: usize,
    data_start: usize,
}

fn parse_header(bytes: &[u8]) -> Result<Header, String> {
    // The header is ASCII; cap the search so a corrupt file cannot make us
    // decode the whole payload as text.
    let probe = &bytes[..bytes.len().min(128 * 1024)];
    let text = String::from_utf8_lossy(probe);
    let end = text
        .find("end_header")
        .ok_or_else(|| "Not a PLY file: no end_header.".to_string())?;
    let nl = text[end..]
        .find('\n')
        .ok_or_else(|| "Malformed PLY header.".to_string())?;
    let data_start = end + nl + 1;
    let header = &text[..end];

    if !header.contains("binary_little_endian") {
        return Err(
            "Only binary little-endian PLY files are supported. Re-export the splat.".into(),
        );
    }

    let mut vertex_count = 0usize;
    let mut props = Vec::new();
    let mut offset = 0usize;
    let mut in_vertex = false;
    for line in header.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts.as_slice() {
            ["element", name, count] => {
                in_vertex = *name == "vertex";
                if in_vertex {
                    vertex_count = count.parse().map_err(|_| "Bad vertex count.".to_string())?;
                }
            }
            ["property", ty, name] if in_vertex => {
                let ty = ScalarType::parse(ty)
                    .ok_or_else(|| format!("Unsupported PLY property type '{ty}'."))?;
                props.push(Prop {
                    name: (*name).to_string(),
                    offset,
                    ty,
                });
                offset += ty.size();
            }
            ["property", "list", ..] if in_vertex => {
                return Err("PLY list properties are not supported on vertices.".into());
            }
            _ => {}
        }
    }
    Ok(Header {
        vertex_count,
        props,
        stride: offset,
        data_start,
    })
}

/// Read a 3DGS PLY. Plain point clouds (x/y/z plus optional red/green/blue)
/// are accepted and lifted into small isotropic splats.
pub fn read_ply(path: &Path) -> Result<SplatCloud, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| format!("Cannot open {}: {e}", path.display()))?
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    read_ply_bytes(&bytes)
}

pub fn read_ply_bytes(bytes: &[u8]) -> Result<SplatCloud, String> {
    let h = parse_header(bytes)?;
    let n = h.vertex_count;
    let needed = h
        .data_start
        .checked_add(n.checked_mul(h.stride).ok_or("PLY too large.")?)
        .ok_or("PLY too large.")?;
    if bytes.len() < needed {
        return Err(format!(
            "PLY is truncated: expected {needed} bytes, found {}.",
            bytes.len()
        ));
    }

    let find = |name: &str| h.props.iter().find(|p| p.name == name);
    let get = |body: &[u8], p: &Prop| p.ty.read(&body[p.offset..]);

    for req in ["x", "y", "z"] {
        if find(req).is_none() {
            return Err(format!("PLY is missing the '{req}' property."));
        }
    }

    let rest_per_channel = {
        let mut k = 0usize;
        while find(&format!("f_rest_{k}")).is_some() {
            k += 1;
        }
        if k % 3 != 0 {
            return Err(format!("PLY has {k} f_rest properties, which is not a multiple of 3."));
        }
        k / 3
    };

    let is_splat = find("scale_0").is_some()
        && find("rot_0").is_some()
        && find("opacity").is_some()
        && find("f_dc_0").is_some();

    let mut cloud = SplatCloud {
        positions: Vec::with_capacity(n),
        scales_log: Vec::with_capacity(n),
        rot_wxyz: Vec::with_capacity(n),
        opacity_logit: Vec::with_capacity(n),
        sh_dc: Vec::with_capacity(n),
        sh_rest: Vec::with_capacity(if is_splat { n * rest_per_channel * 3 } else { 0 }),
        rest_per_channel: if is_splat { rest_per_channel } else { 0 },
    };

    // Resolve property lookups once rather than per vertex.
    let px = find("x").unwrap();
    let py = find("y").unwrap();
    let pz = find("z").unwrap();
    let rest_props: Vec<&Prop> = if is_splat {
        (0..rest_per_channel * 3)
            .map(|k| find(&format!("f_rest_{k}")).unwrap())
            .collect()
    } else {
        Vec::new()
    };

    for i in 0..n {
        let body = &bytes[h.data_start + i * h.stride..];
        cloud
            .positions
            .push([get(body, px), get(body, py), get(body, pz)]);

        if is_splat {
            let sc = |name: &str| get(body, find(name).unwrap());
            cloud
                .scales_log
                .push([sc("scale_0"), sc("scale_1"), sc("scale_2")]);
            cloud
                .rot_wxyz
                .push([sc("rot_0"), sc("rot_1"), sc("rot_2"), sc("rot_3")]);
            cloud.opacity_logit.push(sc("opacity"));
            cloud
                .sh_dc
                .push([sc("f_dc_0"), sc("f_dc_1"), sc("f_dc_2")]);
            for p in &rest_props {
                cloud.sh_rest.push(get(body, p));
            }
        } else {
            // A bare point cloud: give every point a small isotropic, opaque
            // splat so the same code paths render and export it.
            cloud.scales_log.push([(5e-3f32).ln(); 3]);
            cloud.rot_wxyz.push([1.0, 0.0, 0.0, 0.0]);
            cloud.opacity_logit.push(4.0);
            let rgb = match (find("red"), find("green"), find("blue")) {
                (Some(r), Some(g), Some(b)) => [get(body, r), get(body, g), get(body, b)],
                _ => [128.0, 128.0, 128.0],
            };
            // Invert the base-colour transform: c = 0.5 + SH_C0 * dc.
            cloud.sh_dc.push([
                (rgb[0] / 255.0 - 0.5) / super::SH_C0,
                (rgb[1] / 255.0 - 0.5) / super::SH_C0,
                (rgb[2] / 255.0 - 0.5) / super::SH_C0,
            ]);
        }
    }

    Ok(cloud)
}

/// Write a 3DGS PLY in the reference property order.
pub fn write_ply(path: &Path, cloud: &SplatCloud) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let file = File::create(path).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    let mut w = BufWriter::new(file);
    write_ply_to(&mut w, cloud).map_err(|e| e.to_string())?;
    w.flush().map_err(|e| e.to_string())
}

pub fn write_ply_to<W: Write>(w: &mut W, cloud: &SplatCloud) -> std::io::Result<()> {
    let n = cloud.len();
    let rest = cloud.rest_per_channel * 3;

    writeln!(w, "ply")?;
    writeln!(w, "format binary_little_endian 1.0")?;
    writeln!(w, "comment Generated by InstaSplatter")?;
    writeln!(w, "element vertex {n}")?;
    for p in ["x", "y", "z", "nx", "ny", "nz"] {
        writeln!(w, "property float {p}")?;
    }
    for k in 0..3 {
        writeln!(w, "property float f_dc_{k}")?;
    }
    for k in 0..rest {
        writeln!(w, "property float f_rest_{k}")?;
    }
    writeln!(w, "property float opacity")?;
    for k in 0..3 {
        writeln!(w, "property float scale_{k}")?;
    }
    for k in 0..4 {
        writeln!(w, "property float rot_{k}")?;
    }
    writeln!(w, "end_header")?;

    let mut buf: Vec<u8> = Vec::with_capacity(n * (17 + rest) * 4);
    for i in 0..n {
        let mut put = |v: f32| buf.extend_from_slice(&v.to_le_bytes());
        for k in 0..3 {
            put(cloud.positions[i][k]);
        }
        for _ in 0..3 {
            put(0.0); // normals are unused by 3DGS but expected by many readers
        }
        for k in 0..3 {
            put(cloud.sh_dc[i][k]);
        }
        for k in 0..rest {
            put(cloud.sh_rest[i * rest + k]);
        }
        put(cloud.opacity_logit[i]);
        for k in 0..3 {
            put(cloud.scales_log[i][k]);
        }
        for k in 0..4 {
            put(cloud.rot_wxyz[i][k]);
        }
    }
    w.write_all(&buf)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cloud(n: usize, rest_per_channel: usize) -> SplatCloud {
        let mut c = SplatCloud {
            rest_per_channel,
            ..Default::default()
        };
        for i in 0..n {
            let f = i as f32;
            c.positions.push([f, f * 2.0, -f]);
            c.scales_log.push([-1.0 + f, -2.0, -3.0]);
            c.rot_wxyz.push([1.0, 0.1 * f, 0.0, 0.0]);
            c.opacity_logit.push(f * 0.25 - 1.0);
            c.sh_dc.push([0.1 * f, -0.2, 0.3]);
            for k in 0..rest_per_channel * 3 {
                c.sh_rest.push(k as f32 * 0.01 + f);
            }
        }
        c
    }

    #[test]
    fn ply_roundtrip_preserves_every_field() {
        for rest in [0usize, 3, 15] {
            let cloud = sample_cloud(7, rest);
            let mut bytes = Vec::new();
            write_ply_to(&mut bytes, &cloud).unwrap();
            let back = read_ply_bytes(&bytes).unwrap();

            assert_eq!(back.len(), cloud.len());
            assert_eq!(back.rest_per_channel, rest);
            assert_eq!(back.positions, cloud.positions);
            assert_eq!(back.scales_log, cloud.scales_log);
            assert_eq!(back.rot_wxyz, cloud.rot_wxyz);
            assert_eq!(back.opacity_logit, cloud.opacity_logit);
            assert_eq!(back.sh_dc, cloud.sh_dc);
            assert_eq!(back.sh_rest, cloud.sh_rest);
        }
    }

    #[test]
    fn sh_degree_is_derived_from_rest_count() {
        assert_eq!(sample_cloud(1, 0).sh_degree(), 0);
        assert_eq!(sample_cloud(1, 3).sh_degree(), 1);
        assert_eq!(sample_cloud(1, 8).sh_degree(), 2);
        assert_eq!(sample_cloud(1, 15).sh_degree(), 3);
    }

    /// A bare `x y z red green blue` cloud, the shape COLMAP and most scanners
    /// emit.
    fn point_cloud_bytes(points: &[([f32; 3], [u8; 3])]) -> Vec<u8> {
        let mut out = format!(
            "ply\nformat binary_little_endian 1.0\nelement vertex {}\n\
             property float x\nproperty float y\nproperty float z\n\
             property uchar red\nproperty uchar green\nproperty uchar blue\nend_header\n",
            points.len()
        )
        .into_bytes();
        for (p, c) in points {
            for v in p {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(c);
        }
        out
    }

    #[test]
    fn point_cloud_ply_is_read_as_opaque_splats() {
        let bytes = point_cloud_bytes(&[
            ([1.0, 2.0, 3.0], [255, 0, 0]),
            ([-1.0, 0.0, 0.5], [0, 128, 255]),
        ]);
        let c = read_ply_bytes(&bytes).unwrap();

        assert_eq!(c.len(), 2);
        assert_eq!(c.rest_per_channel, 0);
        assert_eq!(c.positions[0], [1.0, 2.0, 3.0]);

        // Colour survives the round-trip into the zeroth SH band.
        let base = |i: usize, k: usize| 0.5 + super::super::SH_C0 * c.sh_dc[i][k];
        assert!((base(0, 0) - 1.0).abs() < 1e-3, "{}", base(0, 0));
        assert!(base(0, 1).abs() < 1e-3, "{}", base(0, 1));
        assert!((base(1, 2) - 1.0).abs() < 1e-3, "{}", base(1, 2));
        assert!(c.opacity(1) > 0.9);
    }

    #[test]
    fn truncated_ply_is_rejected() {
        let cloud = sample_cloud(4, 0);
        let mut bytes = Vec::new();
        write_ply_to(&mut bytes, &cloud).unwrap();
        bytes.truncate(bytes.len() - 10);
        let err = read_ply_bytes(&bytes).unwrap_err();
        assert!(err.contains("truncated"), "{err}");
    }

    #[test]
    fn ascii_ply_is_rejected_with_a_clear_message() {
        let bytes = b"ply\nformat ascii 1.0\nelement vertex 0\nend_header\n";
        let err = read_ply_bytes(bytes).unwrap_err();
        assert!(err.contains("binary little-endian"), "{err}");
    }
}
