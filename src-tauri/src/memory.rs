//! Memory snapshot from sysctl / vm_stat / memory_pressure / ps.

use crate::system::run_cmd;
use crate::types::{HistPoint, MemoryInfo, TopProc};

fn parse_swap_size(s: &str) -> u64 {
    let s = s.trim();
    let (num, unit) = s.split_at(s.len().saturating_sub(1));
    let n: f64 = num.parse().unwrap_or(0.0);
    let mult = match unit {
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => 1024.0 * 1024.0 * 1024.0,
        _ => return s.parse::<f64>().unwrap_or(0.0) as u64,
    };
    (n * mult) as u64
}

/// `vm.swapusage: total = 2048.00M  used = 1234.50M  free = 813.50M  (encrypted)` -> (used, total)
pub fn parse_swapusage(s: &str) -> (u64, u64) {
    let mut used = 0;
    let mut total = 0;
    for part in s.split("  ") {
        if let Some((k, v)) = part.split_once('=') {
            let key = k.trim().rsplit(' ').next().unwrap_or("").trim();
            let val = parse_swap_size(v.trim().split(' ').next().unwrap_or("0"));
            match key {
                "total" => total = val,
                "used" => used = val,
                _ => {}
            }
        }
    }
    (used, total)
}

pub struct VmStat {
    pub page_size: u64,
    pub free_pages: u64,
    pub compressor_pages: u64,
    pub pageouts: u64,
}

pub fn parse_vm_stat(s: &str) -> VmStat {
    let mut v = VmStat { page_size: 16384, free_pages: 0, compressor_pages: 0, pageouts: 0 };
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("Mach Virtual Memory Statistics: (page size of ") {
            v.page_size = rest.split(' ').next().and_then(|n| n.parse().ok()).unwrap_or(v.page_size);
            continue;
        }
        let Some((k, val)) = line.split_once(':') else { continue };
        let n: u64 = val.trim().trim_end_matches('.').parse().unwrap_or(0);
        match k.trim() {
            "Pages free" => v.free_pages += n,
            "Pages speculative" => v.free_pages += n,
            "Pages occupied by compressor" => v.compressor_pages = n,
            "Pageouts" => v.pageouts = n,
            _ => {}
        }
    }
    v
}

/// `System-wide memory free percentage: 59%`
pub fn parse_memory_pressure(s: &str) -> Option<f64> {
    s.lines()
        .find(|l| l.contains("memory free percentage"))
        .and_then(|l| l.rsplit(':').next())
        .and_then(|v| v.trim().trim_end_matches('%').parse().ok())
}

pub fn parse_ps(s: &str, limit: usize) -> Vec<TopProc> {
    s.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let pid: u32 = it.next()?.parse().ok()?;
            let rss_kb: u64 = it.next()?.parse().ok()?;
            let comm = it.collect::<Vec<_>>().join(" ");
            let name = comm.rsplit('/').next().unwrap_or(&comm).to_string();
            Some(TopProc { pid, rss: rss_kb * 1024, name })
        })
        .take(limit)
        .collect()
}

/// `{ sec = 1761000000, usec = 0 } Mon Nov  3 ...`
pub fn parse_boottime(s: &str) -> Option<i64> {
    let i = s.find("sec = ")? + 6;
    s[i..].split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

fn pressure_level(free_pct: f64) -> &'static str {
    match run_cmd("sysctl", &["-n", "kern.memorystatus_vm_pressure_level"])
        .and_then(|s| s.trim().parse::<u32>().ok())
    {
        Some(1) => "normal",
        Some(2) => "warn",
        Some(4) => "critical",
        _ if free_pct < 10.0 => "critical",
        _ if free_pct < 25.0 => "warn",
        _ => "normal",
    }
}

pub fn snapshot(history: Vec<HistPoint>) -> MemoryInfo {
    let total_bytes =
        run_cmd("sysctl", &["-n", "hw.memsize"]).and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    let (swap_used, swap_total) =
        run_cmd("sysctl", &["vm.swapusage"]).map(|s| parse_swapusage(&s)).unwrap_or((0, 0));
    let vm = run_cmd("vm_stat", &[]).map(|s| parse_vm_stat(&s)).unwrap_or(VmStat {
        page_size: 16384,
        free_pages: 0,
        compressor_pages: 0,
        pageouts: 0,
    });
    let free_pct =
        run_cmd("memory_pressure", &[]).and_then(|s| parse_memory_pressure(&s)).unwrap_or_else(|| {
            if total_bytes > 0 {
                (vm.free_pages * vm.page_size) as f64 * 100.0 / total_bytes as f64
            } else {
                0.0
            }
        });
    let top = run_cmd("ps", &["-axm", "-o", "pid=,rss=,comm="]).map(|s| parse_ps(&s, 15)).unwrap_or_default();
    let boot = run_cmd("sysctl", &["-n", "kern.boottime"]).and_then(|s| parse_boottime(&s)).unwrap_or(0);
    let uptime_sec = (crate::scanner::now() - boot).max(0) as u64;
    MemoryInfo {
        total_bytes,
        free_pct,
        swap_used,
        swap_total,
        compressed_bytes: vm.compressor_pages * vm.page_size,
        pageouts: vm.pageouts,
        pressure: pressure_level(free_pct).to_string(),
        top,
        uptime_sec,
        history,
    }
}

/// Cheap free% + swap reading for the 10s sampler (no ps, no boottime).
pub fn quick_point(ts: i64) -> HistPoint {
    let (swap_used, _) = run_cmd("sysctl", &["vm.swapusage"]).map(|s| parse_swapusage(&s)).unwrap_or((0, 0));
    let free_pct =
        run_cmd("memory_pressure", &[]).and_then(|s| parse_memory_pressure(&s)).unwrap_or_else(|| {
            let total: u64 =
                run_cmd("sysctl", &["-n", "hw.memsize"]).and_then(|s| s.trim().parse().ok()).unwrap_or(0);
            let vm = run_cmd("vm_stat", &[]).map(|s| parse_vm_stat(&s));
            match (vm, total) {
                (Some(vm), t) if t > 0 => (vm.free_pages * vm.page_size) as f64 * 100.0 / t as f64,
                _ => 0.0,
            }
        });
    HistPoint { ts, free_pct, swap_used }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swapusage() {
        let (u, t) =
            parse_swapusage("vm.swapusage: total = 2048.00M  used = 1234.50M  free = 813.50M  (encrypted)");
        assert_eq!(t, 2048 * 1024 * 1024);
        assert_eq!(u, (1234.5 * 1024.0 * 1024.0) as u64);
    }

    #[test]
    fn vm_stat() {
        let v = parse_vm_stat("Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:                               12345.\nPages speculative:                        100.\nPages occupied by compressor:             2000.\nPageouts:                                 77.\n");
        assert_eq!(v.page_size, 16384);
        assert_eq!(v.free_pages, 12445);
        assert_eq!(v.compressor_pages, 2000);
        assert_eq!(v.pageouts, 77);
    }

    #[test]
    fn pressure_ps_boot() {
        assert_eq!(parse_memory_pressure("x\nSystem-wide memory free percentage: 59%\n"), Some(59.0));
        let top = parse_ps(
            "  123  40960 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome\n 4 10 kernel_task\n",
            1,
        );
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].name, "Google Chrome");
        assert_eq!(top[0].rss, 40960 * 1024);
        assert_eq!(parse_boottime("{ sec = 1761000000, usec = 0 } Mon Nov  3"), Some(1761000000));
    }
}
