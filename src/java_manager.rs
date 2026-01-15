use anyhow::{anyhow, Result};
use flate2::read::GzDecoder;
use reqwest;
use serde::Deserialize;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand};
use tar::Archive;

#[derive(Deserialize, Debug)]
struct AdoptiumRelease {
    binaries: Vec<AdoptiumBinary>,
}

#[derive(Deserialize, Debug)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(Deserialize, Debug)]
struct AdoptiumPackage {
    link: String,
}



#[derive(Clone)]
pub struct JavaManager {
    runtimes_dir: PathBuf,
}

impl JavaManager {
    pub fn new(runtimes_dir: PathBuf) -> Self {
        Self { runtimes_dir }
    }

    pub async fn download_and_install_java<F>(&self, version: u32, on_progress: F) -> Result<PathBuf>
    where
        F: Fn(f64, String) + Send + Sync + 'static,
    {
        // 1. Check if already installed (ISOLATED: ONLY CHECK RUNTIMES DIR)
        let target_dir = self.runtimes_dir.join(format!("java-{}", version));
        if target_dir.exists() {
            let java_bin = target_dir.join("bin").join("java");
            if java_bin.exists() {
                return Ok(java_bin);
            }
        }

        on_progress(0.0, format!("Finding Java {}...", version));

        // 2. Fetch Release Info
        // Adoptium API uses "linux"
        let api_os = "linux";

        let url = format!(
            "https://api.adoptium.net/v3/assets/feature_releases/{}/ga?architecture=x64&heap_size=normal&image_type=jdk&jvm_impl=hotspot&os={}",
            version, api_os
        );

        let client = reqwest::Client::new();
        let resp = client.get(&url).send().await?;
        let releases: Vec<AdoptiumRelease> = resp.json().await?;

        if releases.is_empty() {
             return Err(anyhow!("No Java runtimes found for version {}", version));
        }

        let release = &releases[0];
        let binary = &release.binaries[0];
        let download_url = &binary.package.link;

        on_progress(0.1, format!("Downloading Java {}...", version));

        // 3. Download
        let response = client.get(download_url).send().await?;
        let total_size = response.content_length().unwrap_or(0);

        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;
        let mut chunks = Vec::new();

        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            downloaded += chunk.len() as u64;
            chunks.extend_from_slice(&chunk);

            if total_size > 0 {
                let pct = 0.1 + (0.6 * (downloaded as f64 / total_size as f64));
                 on_progress(pct, format!("Downloading Java {}... ({:.1} MB)", version, downloaded as f64 / 1024.0 / 1024.0));
            }
        }

        on_progress(0.7, "Extracting Java Runtime...".to_string());

        // 4. Extract
        // Windows often comes as .zip, Linux as .tar.gz. API might return zip for Windows.
        // Usually Adoptium returns .zip for Windows and .tar.gz for Linux.
        // We need to handle both or ensure we request tar.gz if possible, BUT Windows doesn't handle tar.gz natively easily?
        // Rust's `flate2`/`tar` can handle it fine.
        // Let's check if the URL ends in zip.

        let should_use_zip = download_url.ends_with(".zip");

        let temp_dir = self.runtimes_dir.join(format!("temp_{}", version));
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir)?;
        }
        fs::create_dir_all(&temp_dir)?;

        if should_use_zip {
             let reader = Cursor::new(chunks);
             let mut archive = zip::ZipArchive::new(reader)?;
             archive.extract(&temp_dir)?;
        } else {
             let tar = GzDecoder::new(Cursor::new(chunks));
             let mut archive = Archive::new(tar);
             archive.unpack(&temp_dir)?;
        }

        // Find the extracted folder
        let mut entries = fs::read_dir(&temp_dir)?;
        let extracted_root = if let Some(entry) = entries.next() {
            entry?.path()
        } else {
            return Err(anyhow!("Failed to extract Java: Empty archive"));
        };

        if target_dir.exists() {
            fs::remove_dir_all(&target_dir)?;
        }

        // On Windows rename might fail if crossing drives or locking, but here it's same drive usually.
        // But `fs::rename` sometimes fails for directories on Windows if target exists (we removed it).
        // Let's try standard rename.
        fs::rename(&extracted_root, &target_dir)?;
        fs::remove_dir_all(&temp_dir)?;

        on_progress(1.1, "Java Installed!".to_string());

        let java_bin = target_dir.join("bin").join("java");

        if !java_bin.exists() {
            return Err(anyhow!("Java binary not found in extracted files"));
        }

        // Ensure executable permissions (Unix only)
        #[cfg(target_family = "unix")]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&java_bin)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&java_bin, perms)?;
        }

        Ok(java_bin)
    }

    fn get_java_version(&self, path: &Path) -> Result<u32> {
        let output = StdCommand::new(path)
            .arg("-version")
            .output()?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        // Example output: openjdk version "17.0.1" ...
        // or: java version "1.8.0_..."

        for line in stderr.lines() {
            if line.contains("version") {
                let parts: Vec<&str> = line.split('"').collect();
                if parts.len() >= 2 {
                    let version_str = parts[1];
                    if version_str.starts_with("1.") {
                         // 1.8.0 -> 8
                         if let Some(minor) = version_str.split('.').nth(1) {
                             if let Ok(v) = minor.parse::<u32>() {
                                 return Ok(v);
                             }
                         }
                    } else {
                        // 17.0.1 -> 17
                        if let Some(major) = version_str.split('.').next() {
                            if let Ok(v) = major.parse::<u32>() {
                                return Ok(v);
                            }
                        }
                    }
                }
            }
        }
        anyhow::bail!("Could not parse Java version")
    }

    pub fn find_java(&self, required_version: Option<u32>) -> Result<PathBuf> {
        // Strict Isolation: Only check runtimes directory

        // Add specific version candidates if requirement is known
        if let Some(ver) = required_version {
             // Check runtimes directory (managed java)
             let runtime_java = self.runtimes_dir.join(format!("java-{}", ver)).join("bin").join("java");
             if runtime_java.exists() {
                  return Ok(runtime_java);
             }
        }

        if let Some(req) = required_version {
              return Err(anyhow!("Minecraft requires Java {} but it was not found in runtimes. Attempts to download should have occurred before calling this.", req));
        }

        anyhow::bail!("Could not find Java in runtimes directory")
    }

    #[allow(dead_code)]
    pub fn get_installed_java_versions(&self) -> Vec<String> {
        let mut found_versions = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();

        // 1. Check JAVA_HOME environment variable (Cross-platform)
        if let Ok(java_home) = std::env::var("JAVA_HOME") {
            let path = PathBuf::from(java_home).join("bin").join("java");
            if path.exists() {
                 if let Ok(path_abs) = std::fs::canonicalize(&path) {
                    if seen_paths.insert(path_abs.clone()) {
                        if let Ok(ver) = self.get_java_version(&path_abs) {
                            found_versions.push(format!("Java {} ({})", ver, path_abs.display()));
                        }
                    }
                 }
            }
        }

        // 2. Linux Candidates
         let candidates = vec![
            "java".to_string(),
            "/usr/bin/java".to_string(),
            "/usr/local/bin/java".to_string(),
            "/opt/java/bin/java".to_string(),
            "/usr/lib/jvm/java-8-openjdk-amd64/bin/java".to_string(),
            "/usr/lib/jvm/java-11-openjdk-amd64/bin/java".to_string(),
            "/usr/lib/jvm/java-17-openjdk-amd64/bin/java".to_string(),
            "/usr/lib/jvm/java-21-openjdk-amd64/bin/java".to_string(),
            "/usr/lib/jvm/java-8-openjdk/bin/java".to_string(),
            "/usr/lib/jvm/java-17-openjdk/bin/java".to_string(),
            // Arch Linux common paths
            "/usr/lib/jvm/default/bin/java".to_string(),
        ];

        for path_str in candidates {
            let path = if path_str.contains("/") {
                 PathBuf::from(&path_str)
            } else {
                 if let Ok(output) = StdCommand::new("which").arg(&path_str).output() {
                    if output.status.success() {
                        let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
                         PathBuf::from(p)
                    } else {
                        continue;
                    }
                 } else {
                     continue;
                 }
            };

            if path.exists() {
                 if let Ok(path_abs) = std::fs::canonicalize(&path) {
                     if seen_paths.contains(&path_abs) {
                         continue;
                     }
                     seen_paths.insert(path_abs.clone());

                     // Get version
                     if let Ok(ver) = self.get_java_version(&path_abs) {
                          found_versions.push(format!("Java {} ({})", ver, path_abs.display()));
                     }
                 }
            }
        }

        // Try scanning /usr/lib/jvm for other versions
        if let Ok(entries) = std::fs::read_dir("/usr/lib/jvm") {
            for entry in entries.flatten() {
                let path = entry.path().join("bin").join("java");
                if path.exists() {
                    if let Ok(path_abs) = std::fs::canonicalize(&path) {
                         if seen_paths.contains(&path_abs) {
                             continue;
                         }
                         seen_paths.insert(path_abs.clone());
                         if let Ok(ver) = self.get_java_version(&path_abs) {
                              found_versions.push(format!("Java {} ({})", ver, path_abs.display()));
                         }
                    }
                }
            }
        }

        found_versions
    }
}
