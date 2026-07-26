//! Hardware profiler: detects GPU / CPU / RAM and resolves an automatic
//! quality preset for this machine (ROADMAP §4).

use serde::{Deserialize, Serialize};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn hidden_command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub gpu_name: String,
    pub gpu_vendor: GpuVendor,
    pub vram_mb: u64,
    pub has_cuda: bool,
    pub cpu_name: String,
    pub cpu_threads: usize,
    pub ram_mb: u64,
    /// Preset chosen automatically for this hardware.
    pub auto_preset: Preset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    Draft,
    Eco,
    Balanced,
    High,
    Max,
}

/// Concrete pipeline parameters a preset expands to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetParams {
    pub max_frames: u32,
    pub max_resolution: u32,
    pub total_steps: u32,
    pub max_splats: u32,
    pub sh_degree: u32,
    pub export_every: u32,
    pub refine_every: u32,
}

impl Preset {
    pub fn params(self) -> PresetParams {
        match self {
            Preset::Draft => PresetParams {
                max_frames: 80,
                max_resolution: 1024,
                total_steps: 5_000,
                max_splats: 1_500_000,
                sh_degree: 2,
                // Fewer exports: the viewport interpolates between checkpoints.
                export_every: 500,
                refine_every: 100,
            },
            Preset::Eco => PresetParams {
                max_frames: 120,
                max_resolution: 1280,
                total_steps: 10_000,
                max_splats: 2_500_000,
                sh_degree: 2,
                export_every: 750,
                refine_every: 100,
            },
            Preset::Balanced => PresetParams {
                max_frames: 180,
                max_resolution: 1440,
                total_steps: 20_000,
                max_splats: 4_000_000,
                sh_degree: 3,
                export_every: 1_000,
                refine_every: 100,
            },
            Preset::High => PresetParams {
                max_frames: 260,
                max_resolution: 1600,
                total_steps: 35_000,
                max_splats: 6_000_000,
                sh_degree: 3,
                export_every: 1_500,
                refine_every: 100,
            },
            Preset::Max => PresetParams {
                max_frames: 400,
                max_resolution: 1920,
                total_steps: 60_000,
                max_splats: 12_000_000,
                sh_degree: 3,
                export_every: 2_000,
                refine_every: 80,
            },
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Preset> {
        match s.to_ascii_lowercase().as_str() {
            "draft" => Some(Preset::Draft),
            "eco" => Some(Preset::Eco),
            "balanced" => Some(Preset::Balanced),
            "high" => Some(Preset::High),
            "max" => Some(Preset::Max),
            _ => None,
        }
    }
}

fn detect_nvidia() -> Option<(String, u64)> {
    let out = hidden_command("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().next()?.trim();
    let (name, mem) = line.rsplit_once(',')?;
    let vram: u64 = mem.trim().parse().ok()?;
    Some((name.trim().to_string(), vram))
}

/// True when the NVIDIA driver stack answers (`nvidia-smi`). That is the
/// practical gate for CUDA COLMAP / gsplat — a WMI-only NVIDIA name without a
/// working driver is not enough to enable GPU feature extraction.
pub fn cuda_driver_available() -> bool {
    detect_nvidia().is_some()
}

fn detect_gpu_wmi() -> Option<String> {
    let out = hidden_command("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_VideoController | Where-Object {$_.Name -notmatch 'Virtual|Basic'} | Select-Object -First 1).Name",
        ])
        .output()
        .ok()?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn vendor_of(name: &str) -> GpuVendor {
    let n = name.to_ascii_lowercase();
    if n.contains("nvidia") || n.contains("geforce") || n.contains("rtx") || n.contains("gtx") {
        GpuVendor::Nvidia
    } else if n.contains("amd") || n.contains("radeon") {
        GpuVendor::Amd
    } else if n.contains("intel") || n.contains("arc") || n.contains("iris") || n.contains("uhd") {
        GpuVendor::Intel
    } else {
        GpuVendor::Unknown
    }
}

fn auto_preset(vendor: GpuVendor, vram_mb: u64, ram_mb: u64) -> Preset {
    if vendor == GpuVendor::Unknown || vram_mb == 0 {
        return Preset::Draft;
    }
    if ram_mb < 12_000 {
        return Preset::Eco;
    }
    match vram_mb {
        0..=3_500 => Preset::Draft,
        3_501..=6_500 => Preset::Balanced,
        6_501..=11_500 => Preset::High,
        _ => Preset::Max,
    }
}

pub fn profile() -> HardwareProfile {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.refresh_cpu_all();
    let ram_mb = sys.total_memory() / (1024 * 1024);
    let cpu_threads = sys.cpus().len().max(1);
    let cpu_name = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_else(|| "Unknown CPU".into());

    // Prefer nvidia-smi (driver + CUDA usable). Fall back to WMI for naming
    // only — an NVIDIA name without nvidia-smi means drivers are missing.
    let (gpu_name, vram_mb, has_cuda) = if let Some((name, vram)) = detect_nvidia() {
        (name, vram, true)
    } else if let Some(name) = detect_gpu_wmi() {
        let vendor = vendor_of(&name);
        let nvidia_without_driver = vendor == GpuVendor::Nvidia;
        (
            if nvidia_without_driver {
                format!("{name} (NVIDIA driver / nvidia-smi not available)")
            } else {
                name
            },
            4_096,
            false,
        )
    } else {
        ("Unknown GPU".to_string(), 0, false)
    };

    let gpu_vendor = vendor_of(&gpu_name);
    let auto = auto_preset(gpu_vendor, vram_mb, ram_mb);

    HardwareProfile {
        gpu_name,
        gpu_vendor,
        vram_mb,
        has_cuda,
        cpu_name,
        cpu_threads,
        ram_mb,
        auto_preset: auto,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_detects_common_nvidia_names() {
        assert_eq!(vendor_of("NVIDIA GeForce RTX 4060"), GpuVendor::Nvidia);
        assert_eq!(vendor_of("GeForce GTX 1080"), GpuVendor::Nvidia);
        assert_eq!(vendor_of("AMD Radeon RX 6800"), GpuVendor::Amd);
        assert_eq!(vendor_of("Intel Arc A770"), GpuVendor::Intel);
    }

    #[test]
    fn auto_preset_scales_with_vram() {
        assert_eq!(auto_preset(GpuVendor::Nvidia, 4_000, 32_000), Preset::Balanced);
        assert_eq!(auto_preset(GpuVendor::Nvidia, 8_000, 32_000), Preset::High);
        assert_eq!(auto_preset(GpuVendor::Nvidia, 16_000, 32_000), Preset::Max);
        assert_eq!(auto_preset(GpuVendor::Unknown, 16_000, 32_000), Preset::Draft);
    }
}
