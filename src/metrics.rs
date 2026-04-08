use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    fs,
    process::Command,
    sync::{Mutex, OnceLock},
    time::Duration,
};

#[derive(Debug, Clone, Default)]
pub struct SystemMetrics {
    pub hostname: String,
    pub ip_addr: String,
    pub uptime_text: String,
    pub ram_percent_text: String,
    pub cpu_temp_text: String,
    pub cpu_usage_percent_text: String,
    pub load_avg_1: String,
    pub load_avg_5: String,
    pub load_avg_15: String,
    pub load_avg_text: String,
    pub mem_total_mib_text: String,
    pub mem_used_mib_text: String,
    pub mem_available_mib_text: String,
    pub mem_free_mib_text: String,
    pub swap_total_mib_text: String,
    pub swap_used_mib_text: String,
    pub swap_free_mib_text: String,
    pub swap_used_percent_text: String,
    pub procs_running_text: String,
    pub procs_blocked_text: String,
    pub cpu_cores_text: String,
    pub os_pretty_name: String,
    pub kernel_release: String,
}

pub fn collect_metrics() -> Result<SystemMetrics> {
    let memory = get_memory_snapshot().ok();
    let (load1, load5, load15) = get_load_averages()
        .unwrap_or_else(|_| ("N/A".to_string(), "N/A".to_string(), "N/A".to_string()));

    Ok(SystemMetrics {
        hostname: get_hostname().unwrap_or_else(|_| "unknown".to_string()),
        ip_addr: get_ip().unwrap_or_else(|_| "No IP".to_string()),
        uptime_text: get_uptime_text().unwrap_or_else(|_| "N/A".to_string()),
        ram_percent_text: memory
            .as_ref()
            .map(|m| m.used_percent_text.clone())
            .unwrap_or_else(|| "N/A".to_string()),
        cpu_temp_text: get_cpu_temp_text().unwrap_or_else(|_| "N/A".to_string()),
        cpu_usage_percent_text: get_cpu_usage_percent_text().unwrap_or_else(|_| "N/A".to_string()),
        load_avg_1: load1.clone(),
        load_avg_5: load5.clone(),
        load_avg_15: load15.clone(),
        load_avg_text: format!("{load1} {load5} {load15}"),
        mem_total_mib_text: memory
            .as_ref()
            .map(|m| m.total_mib_text.clone())
            .unwrap_or_else(|| "N/A".to_string()),
        mem_used_mib_text: memory
            .as_ref()
            .map(|m| m.used_mib_text.clone())
            .unwrap_or_else(|| "N/A".to_string()),
        mem_available_mib_text: memory
            .as_ref()
            .map(|m| m.available_mib_text.clone())
            .unwrap_or_else(|| "N/A".to_string()),
        mem_free_mib_text: memory
            .as_ref()
            .map(|m| m.free_mib_text.clone())
            .unwrap_or_else(|| "N/A".to_string()),
        swap_total_mib_text: memory
            .as_ref()
            .map(|m| m.swap_total_mib_text.clone())
            .unwrap_or_else(|| "N/A".to_string()),
        swap_used_mib_text: memory
            .as_ref()
            .map(|m| m.swap_used_mib_text.clone())
            .unwrap_or_else(|| "N/A".to_string()),
        swap_free_mib_text: memory
            .as_ref()
            .map(|m| m.swap_free_mib_text.clone())
            .unwrap_or_else(|| "N/A".to_string()),
        swap_used_percent_text: memory
            .as_ref()
            .map(|m| m.swap_used_percent_text.clone())
            .unwrap_or_else(|| "N/A".to_string()),
        procs_running_text: get_proc_stat_value("procs_running")
            .unwrap_or_else(|_| "N/A".to_string()),
        procs_blocked_text: get_proc_stat_value("procs_blocked")
            .unwrap_or_else(|_| "N/A".to_string()),
        cpu_cores_text: std::thread::available_parallelism()
            .map(|n| n.get().to_string())
            .unwrap_or_else(|_| "N/A".to_string()),
        os_pretty_name: get_os_pretty_name().unwrap_or_else(|_| "N/A".to_string()),
        kernel_release: fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|v| v.trim().to_string())
            .unwrap_or_else(|_| "N/A".to_string()),
    })
}

fn get_hostname() -> Result<String> {
    let raw = fs::read_to_string("/etc/hostname").context("failed to read /etc/hostname")?;
    Ok(raw.trim().to_string())
}

fn get_ip() -> Result<String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg("hostname -I | awk '{print $1}'")
        .output()
        .context("failed to run hostname -I")?;

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if text.is_empty() {
        Ok("No IP".to_string())
    } else {
        Ok(text)
    }
}

fn get_uptime_text() -> Result<String> {
    let raw = fs::read_to_string("/proc/uptime").context("failed to read /proc/uptime")?;

    let first = raw
        .split_whitespace()
        .next()
        .context("missing uptime value")?;

    let uptime_seconds: f64 = first.parse().context("failed to parse uptime seconds")?;
    let uptime = Duration::from_secs_f64(uptime_seconds);

    let total_minutes = uptime.as_secs() / 60;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;

    Ok(format!("{hours}h {minutes}m"))
}

fn parse_meminfo_kib(value: &str) -> Option<f64> {
    value
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())
}

#[derive(Debug, Clone)]
struct MemorySnapshot {
    total_mib_text: String,
    used_mib_text: String,
    available_mib_text: String,
    free_mib_text: String,
    used_percent_text: String,
    swap_total_mib_text: String,
    swap_used_mib_text: String,
    swap_free_mib_text: String,
    swap_used_percent_text: String,
}

fn kib_to_mib_text(kib: f64) -> String {
    format!("{:.0}MiB", kib / 1024.0)
}

fn percent_text(numerator: f64, denominator: f64) -> String {
    if denominator <= 0.0 {
        "0%".to_string()
    } else {
        format!("{:.0}%", (numerator / denominator) * 100.0)
    }
}

fn get_memory_snapshot() -> Result<MemorySnapshot> {
    let raw = fs::read_to_string("/proc/meminfo").context("failed to read /proc/meminfo")?;
    let mut values: HashMap<String, f64> = HashMap::new();

    for line in raw.lines() {
        if let Some((key, value_part)) = line.split_once(':') {
            if let Some(value) = parse_meminfo_kib(value_part) {
                values.insert(key.trim().to_string(), value);
            }
        }
    }

    let total = *values.get("MemTotal").context("MemTotal not found")?;
    let available = *values
        .get("MemAvailable")
        .context("MemAvailable not found")?;
    let free = *values.get("MemFree").unwrap_or(&0.0);
    let used = (total - available).max(0.0);

    let swap_total = *values.get("SwapTotal").unwrap_or(&0.0);
    let swap_free = *values.get("SwapFree").unwrap_or(&0.0);
    let swap_used = (swap_total - swap_free).max(0.0);

    Ok(MemorySnapshot {
        total_mib_text: kib_to_mib_text(total),
        used_mib_text: kib_to_mib_text(used),
        available_mib_text: kib_to_mib_text(available),
        free_mib_text: kib_to_mib_text(free),
        used_percent_text: percent_text(used, total),
        swap_total_mib_text: kib_to_mib_text(swap_total),
        swap_used_mib_text: kib_to_mib_text(swap_used),
        swap_free_mib_text: kib_to_mib_text(swap_free),
        swap_used_percent_text: percent_text(swap_used, swap_total),
    })
}

fn get_cpu_temp_text() -> Result<String> {
    let thermal_paths = [
        "/sys/class/thermal/thermal_zone0/temp",
        "/sys/devices/virtual/thermal/thermal_zone0/temp",
    ];

    for path in thermal_paths {
        if let Ok(raw) = fs::read_to_string(path) {
            if let Ok(temp_milli_c) = raw.trim().parse::<f64>() {
                let temp_c = temp_milli_c / 1000.0;
                return Ok(format!("{temp_c:.1}C"));
            }
        }
    }

    Ok("N/A".to_string())
}

fn get_load_averages() -> Result<(String, String, String)> {
    let raw = fs::read_to_string("/proc/loadavg").context("failed to read /proc/loadavg")?;
    let mut parts = raw.split_whitespace();
    let avg1 = parts.next().context("load avg 1m missing")?.to_string();
    let avg5 = parts.next().context("load avg 5m missing")?.to_string();
    let avg15 = parts.next().context("load avg 15m missing")?.to_string();
    Ok((avg1, avg5, avg15))
}

#[derive(Debug, Clone, Copy)]
struct CpuSample {
    total: u64,
    idle: u64,
}

static CPU_SAMPLE_STATE: OnceLock<Mutex<Option<CpuSample>>> = OnceLock::new();

fn get_cpu_usage_percent_text() -> Result<String> {
    let sample = read_cpu_sample()?;
    let state = CPU_SAMPLE_STATE.get_or_init(|| Mutex::new(None));
    let mut guard = state
        .lock()
        .map_err(|_| anyhow::anyhow!("cpu sample state lock poisoned"))?;

    let usage = if let Some(prev) = *guard {
        let total_delta = sample.total.saturating_sub(prev.total);
        let idle_delta = sample.idle.saturating_sub(prev.idle);
        if total_delta == 0 {
            0.0
        } else {
            ((total_delta.saturating_sub(idle_delta)) as f64 / total_delta as f64) * 100.0
        }
    } else {
        0.0
    };

    *guard = Some(sample);
    Ok(format!("{usage:.0}%"))
}

fn read_cpu_sample() -> Result<CpuSample> {
    let raw = fs::read_to_string("/proc/stat").context("failed to read /proc/stat")?;
    let first_line = raw.lines().next().context("missing /proc/stat cpu line")?;
    let mut parts = first_line.split_whitespace();
    let label = parts.next().context("missing cpu label")?;
    anyhow::ensure!(label == "cpu", "unexpected first /proc/stat label: {label}");

    let values: Vec<u64> = parts.filter_map(|p| p.parse::<u64>().ok()).collect();
    anyhow::ensure!(values.len() >= 4, "insufficient cpu stat fields");

    let idle = values.get(3).copied().unwrap_or(0) + values.get(4).copied().unwrap_or(0);
    let total: u64 = values.iter().copied().sum();

    Ok(CpuSample { total, idle })
}

fn get_proc_stat_value(key: &str) -> Result<String> {
    let raw = fs::read_to_string("/proc/stat").context("failed to read /proc/stat")?;
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix(key) {
            let value = rest
                .split_whitespace()
                .next()
                .context("missing proc stat value")?;
            return Ok(value.to_string());
        }
    }
    Err(anyhow::anyhow!("proc stat key not found: {key}"))
}

fn get_os_pretty_name() -> Result<String> {
    let raw = fs::read_to_string("/etc/os-release").context("failed to read /etc/os-release")?;
    for line in raw.lines() {
        if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
            return Ok(value.trim_matches('"').to_string());
        }
    }
    Err(anyhow::anyhow!("PRETTY_NAME not found in /etc/os-release"))
}
