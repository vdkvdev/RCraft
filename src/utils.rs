use anyhow::Result;
use std::cmp::Ordering;

use crate::models::Library;

pub fn parse_version(s: &str) -> (i32, i32, i32) {
    let parts: Vec<&str> = s.split('.').collect();
    (
        parts.get(0).unwrap_or(&"0").parse().unwrap_or(0),
        parts.get(1).map_or(0, |x| x.parse().unwrap_or(0)),
        parts.get(2).map_or(0, |x| x.parse().unwrap_or(0)),
    )
}

pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let pa = parse_version(a);
    let pb = parse_version(b);
    (pa.0, pa.1, pa.2).cmp(&(pb.0, pb.1, pb.2))
}

pub fn is_at_least_1_8(v: &str) -> bool {
    let version_str = if v.contains("fabric") {
        v.split('-').last().unwrap_or(v)
    } else {
        v
    };
    let p = parse_version(version_str);
    p.0 > 1 || (p.0 == 1 && p.1 >= 8)
}

pub fn is_at_least_1_14(v: &str) -> bool {
    let p = parse_version(v);
    p.0 > 1 || (p.0 == 1 && p.1 >= 14)
}

pub fn get_total_ram_mb() -> Result<u32> {
    let content = std::fs::read_to_string("/proc/meminfo")?;
    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let kb: u64 = parts[1].parse()?;
                return Ok((kb / 1024) as u32);
            }
        }
    }
    anyhow::bail!("Could not read total RAM from /proc/meminfo")
}

pub fn is_library_allowed(lib: &Library, os_name: &str) -> bool {
    let rules = match &lib.rules {
        Some(r) => r,
        None => return true,
    };
    let mut allowed = false;
    for rule in rules {
        let matches = if let Some(os) = &rule.os {
            if let Some(name) = &os.name {
                name == os_name
            } else {
                true
            }
        } else {
            true
        };
        if matches {
            allowed = rule.action == "allow";
        }
    }
    allowed
}
