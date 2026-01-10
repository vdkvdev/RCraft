use anyhow::{anyhow, Result};
use flate2::read::GzDecoder;
use reqwest;
use serde::Deserialize;
use std::fs;
use std::io::Cursor;
use std::path::{PathBuf};
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
        // 1. Check if already installed
        // We look for a folder starting with java-{version}
        // Note: Our installation logic creates folders like `java-17-openjdk-...`
        // But for simplicity of detection, we can rename the extracted folder to `java-{version}` or check inside.
        // Let's stick to standard `runtimes/java-{version}` for simplicity and robustness.
        
        let target_dir = self.runtimes_dir.join(format!("java-{}", version));
        if target_dir.exists() {
            let java_bin = target_dir.join("bin").join("java");
            if java_bin.exists() {
                return Ok(java_bin);
            }
        }

        on_progress(0.0, format!("Finding Java {}...", version));

        // 2. Fetch Release Info
        let url = format!(
            "https://api.adoptium.net/v3/assets/feature_releases/{}/ga?architecture=x64&heap_size=normal&image_type=jdk&jvm_impl=hotspot&os=linux",
            version
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
        let tar = GzDecoder::new(Cursor::new(chunks));
        let mut archive = Archive::new(tar);
        
        // Extract to a temporary directory first or check the top-level folder name
        // Tarballs usually create a root directory like `jdk-17.0.x+y`
        
        // We want to extract to `runtimes_dir/temp_{version}` then move/rename inner dir to `target_dir`
        let temp_dir = self.runtimes_dir.join(format!("temp_{}", version));
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir)?;
        }
        fs::create_dir_all(&temp_dir)?;
        
        archive.unpack(&temp_dir)?;
        
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
        
        fs::rename(&extracted_root, &target_dir)?;
        fs::remove_dir_all(&temp_dir)?;

        on_progress(1.0, "Java Installed!".to_string());

        let java_bin = target_dir.join("bin").join("java");
        if !java_bin.exists() {
            return Err(anyhow!("Java binary not found in extracted files"));
        }
        
        // Ensure executable permissions
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&java_bin)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&java_bin, perms)?;

        Ok(java_bin)
    }
}
