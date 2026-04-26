use serde::{Deserialize, Serialize};
use sysinfo::System;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub os: String,
    pub arch: String,
    pub cpu_name: String,
    pub cpu_cores: usize,
    pub total_memory_gb: f64,
    pub accelerator: Accelerator,
    pub recommended_backend: String,
    pub recommended_n_gpu_layers: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Accelerator {
    AppleSilicon {
        chip: String,
        unified_memory_gb: f64,
    },
    Nvidia {
        name: String,
        vram_gb: f64,
        cuda_version: Option<String>,
    },
    Amd {
        name: String,
        vram_gb: f64,
    },
    IntelArc {
        name: String,
    },
    Cpu,
}

pub fn detect() -> HardwareInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_name = sys
        .cpus()
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_else(|| "Unknown".into());
    let cpu_cores = sys.cpus().len();
    let total_memory_gb = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;

    let accelerator = detect_accelerator(total_memory_gb);
    let (recommended_backend, recommended_n_gpu_layers) = match &accelerator {
        Accelerator::AppleSilicon { .. } => ("metal".to_string(), -1),
        Accelerator::Nvidia { .. } => ("cuda".to_string(), -1),
        Accelerator::Amd { .. } => ("vulkan".to_string(), -1),
        Accelerator::IntelArc { .. } => ("vulkan".to_string(), -1),
        Accelerator::Cpu => ("cpu".to_string(), 0),
    };

    HardwareInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_name,
        cpu_cores,
        total_memory_gb,
        accelerator,
        recommended_backend,
        recommended_n_gpu_layers,
    }
}

#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
fn detect_accelerator(total_mem_gb: f64) -> Accelerator {
    #[cfg(target_os = "macos")]
    if std::env::consts::ARCH == "aarch64" {
        let chip = mac_chip_name().unwrap_or_else(|| "Apple Silicon".to_string());
        return Accelerator::AppleSilicon {
            chip,
            unified_memory_gb: total_mem_gb,
        };
    }

    if let Some(nv) = detect_nvidia() {
        return nv;
    }

    if let Some(amd) = detect_amd() {
        return amd;
    }

    Accelerator::Cpu
}

#[cfg(target_os = "macos")]
fn mac_chip_name() -> Option<String> {
    let out = std::process::Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn detect_nvidia() -> Option<Accelerator> {
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let first = s.lines().next()?;
    let parts: Vec<&str> = first.split(',').map(|p| p.trim()).collect();
    if parts.len() < 2 {
        return None;
    }
    let name = parts[0].to_string();
    let vram_mb: f64 = parts[1].parse().unwrap_or(0.0);
    let vram_gb = vram_mb / 1024.0;
    let cuda_version = std::process::Command::new("nvidia-smi")
        .arg("--query")
        .output()
        .ok()
        .and_then(|o| {
            let out = String::from_utf8_lossy(&o.stdout).into_owned();
            out.lines()
                .find(|l| l.contains("CUDA Version"))
                .map(|l| l.trim().to_string())
        });
    Some(Accelerator::Nvidia {
        name,
        vram_gb,
        cuda_version,
    })
}

fn detect_amd() -> Option<Accelerator> {
    if let Ok(out) = std::process::Command::new("rocm-smi")
        .args(["--showproductname", "--showmeminfo", "vram", "--csv"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            for line in s.lines().skip(1) {
                if !line.trim().is_empty() {
                    let parts: Vec<&str> = line.split(',').collect();
                    let name = parts.get(1).unwrap_or(&"AMD GPU").trim().to_string();
                    let vram_gb = parts
                        .get(2)
                        .and_then(|v| v.trim().parse::<f64>().ok())
                        .map(|b| b / 1024.0 / 1024.0 / 1024.0)
                        .unwrap_or(0.0);
                    return Some(Accelerator::Amd { name, vram_gb });
                }
            }
        }
    }

    if cfg!(target_os = "windows") {
        if let Ok(out) = std::process::Command::new("wmic")
            .args(["path", "win32_VideoController", "get", "name,adapterram"])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            for line in s.lines().skip(1) {
                let l = line.trim();
                if l.to_lowercase().contains("amd") || l.to_lowercase().contains("radeon") {
                    return Some(Accelerator::Amd {
                        name: l.to_string(),
                        vram_gb: 0.0,
                    });
                }
            }
        }
    }

    None
}
