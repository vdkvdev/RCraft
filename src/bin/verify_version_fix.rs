fn parse_version(s: &str) -> (i32, i32, i32) {
    let parts: Vec<&str> = s.split('.').collect();
    (
        parts.get(0).unwrap_or(&"0").parse().unwrap_or(0),
        parts.get(1).map_or(0, |x| x.parse().unwrap_or(0)),
        parts.get(2).map_or(0, |x| x.parse().unwrap_or(0)),
    )
}

fn is_at_least_1_8(v: &str) -> bool {
    // Handle Fabric versions like "fabric-loader-0.14.21-1.19.4"
    let version_str = if v.contains("fabric") {
        v.split('-').last().unwrap_or(v)
    } else {
        v
    };
    let p = parse_version(version_str);
    p.0 > 1 || (p.0 == 1 && p.1 >= 8)
}

fn main() {
    let version = "fabric-loader-0.18.4-1.19.4";
    let supported = is_at_least_1_8(version);
    println!("Version '{}' supported: {}", version, supported);
    assert!(supported, "Fabric version should be supported!");
    
    let vanilla = "1.20.1";
    assert!(is_at_least_1_8(vanilla), "Vanilla 1.20.1 should be supported");
    
    let old = "1.7.10";
    assert!(!is_at_least_1_8(old), "Vanilla 1.7.10 should NOT be supported");
    
    println!("All checks passed.");
}
