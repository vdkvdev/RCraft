// launcher.rs - Minecraft launcher core functionality

use anyhow::{anyhow, Result};
use reqwest;
use serde::Deserialize;
// use std::io::Read; // Unused
// But install_fabric uses output() and reads stderr. output() returns Output which has stderr as Vec<u8>.
// So we don't need Read trait for that. But verify imports.
// warning: unused import: `std::io::Read`

use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio}; 
use tokio::fs;
use tokio::process::Command as TokioCommand; 

use crate::config::LauncherConfig;
use crate::models::{AssetIndex, AssetIndexJson, Library, MinecraftVersion, VersionManifest};
use crate::utils::{is_library_allowed, is_at_least_1_8};

// ============================================================================
// Minecraft Launcher
// ============================================================================

#[derive(Clone)]
pub struct MinecraftLauncher {
    pub config: LauncherConfig,
}

// Internal structs for JSON deserialization
#[derive(Deserialize)]
struct VersionJsonDownloads {
    client: Option<DownloadInfo>,
}

#[derive(Deserialize)]
struct VersionJson {
    downloads: Option<VersionJsonDownloads>,
    libraries: Vec<Library>,
    #[serde(rename = "assetIndex")]
    asset_index: Option<AssetIndex>,
}

#[derive(Deserialize)]
struct DownloadInfo {
    url: String,
}

impl MinecraftLauncher {
    /// Create a new MinecraftLauncher instance
    pub fn new() -> Result<Self> {
        Ok(Self {
            config: LauncherConfig::new()?,
        })
    }

    /// Fetch available Minecraft versions from Mojang's manifest
    pub async fn get_available_versions(&self) -> Result<Vec<MinecraftVersion>> {
        let url = "https://launchermeta.mojang.com/mc/game/version_manifest.json";
        let response = reqwest::get(url).await?;
        let manifest: VersionManifest = response.json().await?;

        let release_versions: Vec<MinecraftVersion> = manifest
            .versions
            .into_iter()
            .filter(|v| v.version_type == "release")
            .collect();

        Ok(release_versions)
    }

    /// Download a specific Minecraft version (jar, libraries, assets)
    pub async fn download_version(&self, version: &MinecraftVersion) -> Result<()> {
        let version_dir = self.config.versions_dir.join(&version.id);
        fs::create_dir_all(&version_dir).await?;
        let natives_dir = version_dir.join("natives");
        fs::create_dir_all(&natives_dir).await?;

        // Download version metadata
        let version_response = reqwest::get(&version.url).await?;
        let version_data = version_response.text().await?;
        let version_file = version_dir.join(format!("{}.json", version.id));
        fs::write(&version_file, &version_data).await?;

        let version_json: VersionJson = serde_json::from_str(&version_data)?;
        
        // Determine client jar URL
        let jar_url = if let Some(downloads) = &version_json.downloads {
            downloads.client.as_ref().map(|c| c.url.clone())
        } else {
            None
        }
        .unwrap_or_else(|| {
            format!(
                "https://s3.amazonaws.com/Minecraft.Download/versions/{}/{}.jar",
                &version.id, &version.id
            )
        });

        // Download client jar
        let jar_path = version_dir.join(format!("{}.jar", version.id));
        let resp = reqwest::get(&jar_url).await?;
        let bytes = resp.bytes().await?.to_vec();
        let mut out = tokio::fs::File::create(&jar_path).await?;
        use tokio::io::AsyncWriteExt;
        out.write_all(&bytes).await?;

        let os_name = "linux";
        let has_natives = std::fs::read_dir(&natives_dir).is_ok_and(|rd| rd.count() > 0);
        if !has_natives {
            if natives_dir.exists() {
                std::fs::remove_dir_all(&natives_dir)?;
            }
            std::fs::create_dir_all(&natives_dir)?;
        }

        // Download libraries
        for lib in &version_json.libraries {
            let allowed = is_library_allowed(lib, os_name);
            if !allowed {
                continue;
            }

            if let Some(downloads) = &lib.downloads {
                if let Some(artifact) = &downloads.artifact {
                    let lib_path = self.config.libraries_dir.join(&artifact.path);
                    if !lib_path.exists() {
                        if let Some(parent) = lib_path.parent() {
                            fs::create_dir_all(parent).await?;
                        }
                        let resp = reqwest::get(&artifact.url).await?;
                        let bytes = resp.bytes().await?.to_vec();
                        let mut out = tokio::fs::File::create(&lib_path).await?;
                        out.write_all(&bytes).await?;
                    }
                }
            }

            // Download and extract natives
            if let Some(natives) = &lib.natives {
                if let Some(classifier) = natives.get(os_name) {
                    if let Some(downloads) = &lib.downloads {
                        if let Some(classifiers) = &downloads.classifiers {
                            if let Some(artifact) = classifiers.get(classifier) {
                                let native_zip_path = version_dir.join(format!("{}.zip", lib.name.replace(":", "_")));
                                if !native_zip_path.exists() {
                                    let resp = reqwest::get(&artifact.url).await?;
                                    let bytes = resp.bytes().await?.to_vec();
                                    let mut out = tokio::fs::File::create(&native_zip_path).await?;
                                    out.write_all(&bytes).await?;
                                    out.flush().await?;
                                    out.sync_all().await?;
                                }

                                if has_natives {
                                    continue;
                                }

                                let mut exclude: Vec<String> = Vec::new();
                                if let Some(extract) = lib.get_extract() {
                                    exclude = extract.exclude.clone();
                                }

                                let extraction_result = (|| {
                                    let file = std::fs::File::open(&native_zip_path)?;
                                    let mut archive = zip::ZipArchive::new(file)?;
                                    for i in 0..archive.len() {
                                        let mut file = archive.by_index(i)?;
                                        let name = file.name().to_string();
                                        let excluded = exclude.iter().any(|ex| name.starts_with(ex));
                                        if excluded || name.ends_with("/") {
                                            continue;
                                        }
                                        let filename = std::path::Path::new(&name)
                                            .file_name()
                                            .and_then(|f| f.to_str())
                                            .unwrap_or(&name)
                                            .to_string();
                                        let outpath = natives_dir.join(&filename);
                                        if let Some(parent) = outpath.parent() {
                                            std::fs::create_dir_all(parent)?;
                                        }
                                        let mut outfile = std::fs::File::create(&outpath)?;
                                        std::io::copy(&mut file, &mut outfile)?;
                                    }
                                    Ok::<(), anyhow::Error>(())
                                })();

                                if let Err(e) = extraction_result {
                                    eprintln!("Warning: Failed to extract natives for {}: {}", lib.name, e);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Download assets
        if let Some(asset_index) = &version_json.asset_index {
            let indexes_dir = self.config.assets_dir.join("indexes");
            fs::create_dir_all(&indexes_dir).await?;
            let index_path = indexes_dir.join(format!("{}.json", asset_index.id));

            let resp = reqwest::get(&asset_index.url).await?;
            let bytes = resp.bytes().await?.to_vec();
            let mut out = tokio::fs::File::create(&index_path).await?;
            out.write_all(&bytes).await?;

            let index_data = String::from_utf8(bytes)?;
            let asset_index_json: AssetIndexJson = serde_json::from_str(&index_data)?;

            for (_key, obj) in asset_index_json.objects {
                let hash_prefix = &obj.hash[0..2];
                let object_dir = self.config.assets_dir.join("objects").join(hash_prefix);
                fs::create_dir_all(&object_dir).await?;
                let object_path = object_dir.join(&obj.hash);

                if !object_path.exists() {
                    let object_url = format!("https://resources.download.minecraft.net/{}/{}", hash_prefix, obj.hash);
                    let resp = reqwest::get(&object_url).await?;
                    let bytes = resp.bytes().await?.to_vec();
                    let mut out = tokio::fs::File::create(&object_path).await?;
                    out.write_all(&bytes).await?;
                }
            }
        }

        Ok(())
    }

    /// Build classpath for a specific version
    pub async fn build_classpath(&self, version: &str) -> Result<String> {
        let version_dir = self.config.versions_dir.join(version);
        let version_file = version_dir.join(format!("{}.json", version));
        let version_data = tokio::fs::read_to_string(&version_file).await?;

        #[derive(Deserialize)]
        struct VersionJson {
            libraries: Vec<Library>,
            #[serde(rename = "inheritsFrom")]
            inherits_from: Option<String>,
        }

        let version_json: VersionJson = serde_json::from_str(&version_data)?;
        let os_name = "linux";
        let mut classpath = Vec::new();

        // 1. Add current libraries
        for lib in &version_json.libraries {
            let allowed = is_library_allowed(lib, os_name);
            if !allowed {
                continue;
            }
            // Logic for Fabric: sometimes path is not in artifact, need to check name/url?
            // Existing logic relies on `downloads.artifact`.
            // Fabric installer usually generates standard json with downloads usually?
            // If not, we might be missing libs. But let's assume standard logic for now.
            if let Some(downloads) = &lib.downloads {
                if let Some(artifact) = &downloads.artifact {
                    let lib_path = self.config.libraries_dir.join(&artifact.path);
                    classpath.push(lib_path);
                }
            } else {
                 // Fallback for libraries without explicit downloads block (common in Fabric/Forge)
                 // Use Maven coordinates from 'name' field "group:artifact:version"
                 // Path: group/artifact/version/artifact-version.jar
                 let parts: Vec<&str> = lib.name.split(':').collect();
                 if parts.len() == 3 {
                     let group = parts[0].replace('.', "/");
                     let artifact = parts[1];
                     let version = parts[2];
                     let path = format!("{}/{}/{}/{}-{}.jar", group, artifact, version, artifact, version);
                     let lib_path = self.config.libraries_dir.join(path);
                     if lib_path.exists() {
                         classpath.push(lib_path);
                     }
                 }
            }
        }

        let mut cp_string = classpath
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(":");

        // 2. Handle Inheritance
        if let Some(parent_id) = version_json.inherits_from {
            let parent_cp = Box::pin(self.build_classpath(&parent_id)).await?;
            if !cp_string.is_empty() {
                cp_string.push(':');
                cp_string.push_str(&parent_cp);
            } else {
                cp_string = parent_cp;
            }
        } else {
             // 3. Add Jar (only if not inheriting)
             let jar_path = version_dir.join(format!("{}.jar", version));
             if !cp_string.is_empty() {
                 cp_string.push(':');
             }
             cp_string.push_str(&jar_path.display().to_string());
        }

        Ok(cp_string)
    }

    /// Launch Minecraft with the given parameters, returning the child process
    pub async fn launch_minecraft(&self, version: &str, username: &str, ram_mb: u32) -> Result<TokioCommand> {
        #[derive(Deserialize)]
        struct VersionJson {
            #[serde(rename = "assetIndex")]
            asset_index: Option<AssetIndex>,
            #[serde(rename = "inheritsFrom")]
            inherits_from: Option<String>,
            #[serde(rename = "mainClass")]
            main_class: Option<String>,
        }
        #[derive(Deserialize)]
        struct AssetIndex {
            id: String,
        }

        // Version compatibility check (modified to handle fabric versions)
        if !is_at_least_1_8(version) {
             return Err(anyhow!("Versions below 1.8 are not supported"));
        }

        let java_path = self.find_java()?;
        let version_dir = self.config.versions_dir.join(version);
        let version_file = version_dir.join(format!("{}.json", version));
        
        // Read version JSON to check inheritance
        let version_data = fs::read_to_string(&version_file).await?;
        let version_json: VersionJson = serde_json::from_str(&version_data)?;

        // Determine JAR path
        let jar_version = version_json.inherits_from.as_deref().unwrap_or(version);
        let jar_dir = self.config.versions_dir.join(jar_version);
        let jar_path = jar_dir.join(format!("{}.jar", jar_version));
        
        // Determine Natives directory
        let natives_version = version_json.inherits_from.as_deref().unwrap_or(version);
        let natives_dir = self.config.versions_dir.join(natives_version).join("natives");

        if !jar_path.exists() {
            return Err(anyhow!("Version JAR not found at: {:?}", jar_path));
        }

        // Inheritance Logic for Main Class and Assets
        let mut main_class = version_json.main_class.clone();
        let mut asset_index_id = version_json.asset_index.as_ref().map(|a| a.id.clone());

        if let Some(parent_id) = &version_json.inherits_from {
            let parent_dir = self.config.versions_dir.join(parent_id);
            let parent_file = parent_dir.join(format!("{}.json", parent_id));
            if parent_file.exists() {
                 let parent_data = fs::read_to_string(&parent_file).await?;
                 let parent_json: VersionJson = serde_json::from_str(&parent_data)?;
                 
                 if main_class.is_none() {
                     main_class = parent_json.main_class;
                 }
                 if asset_index_id.is_none() {
                     asset_index_id = parent_json.asset_index.map(|a| a.id);
                 }
            }
        }

        let main_class = main_class.unwrap_or_else(|| "net.minecraft.client.main.Main".to_string());
        let classpath = self.build_classpath(version).await?; 
        
        let mut command = TokioCommand::new(java_path);
        command
            .arg("-Xmx".to_string() + &ram_mb.to_string() + "M")
            .arg("-Xms".to_string() + &(ram_mb / 2).to_string() + "M")
            .arg("-Djava.library.path=".to_string() + &natives_dir.display().to_string())
            .arg("-cp")
            .arg(classpath)
            .arg(main_class) // Dynamic main class
            .arg("--username")
            .arg(username)
            .arg("--version")
            .arg(version)
            .arg("--gameDir")
            .arg(&self.config.minecraft_dir)
            .arg("--assetsDir")
            .arg(&self.config.assets_dir);

        if let Some(id) = asset_index_id {
            command.arg("--assetIndex").arg(id);
        }

        command
            .arg("--accessToken")
            .arg("0")
            .arg("--userProperties")
            .arg("{}")
            .current_dir(&version_dir)
            .current_dir(&version_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Ok(command)
    }

    /// Find Java executable
    pub fn find_java(&self) -> Result<PathBuf> {
        if let Ok(output) = StdCommand::new("which").arg("java8").output() {
            if output.status.success() {
                let java_path = String::from_utf8(output.stdout)?;
                return Ok(PathBuf::from(java_path.trim()));
            }
        }
        if let Ok(output) = StdCommand::new("which").arg("java").output() {
            if output.status.success() {
                let java_path = String::from_utf8(output.stdout)?;
                return Ok(PathBuf::from(java_path.trim()));
            }
        }
        let common_paths = vec![
            "/usr/lib/jvm/java-8-openjdk-amd64/bin/java",
            "/usr/lib/jvm/java-8-openjdk-i386/bin/java",
            "/usr/bin/java",
            "/usr/local/bin/java",
            "/opt/java/bin/java",
        ];
        for path in common_paths {
            if Path::new(path).exists() {
                return Ok(PathBuf::from(path));
            }
        }
        anyhow::bail!("Could not find installed Java (try installing openjdk-8-jdk)")
    }

    /// Install Fabric for a specific Minecraft version
    pub async fn install_fabric(&self, mc_version: &str) -> Result<String> {
        // 1. Download Fabric Installer
        let installer_url = "https://maven.fabricmc.net/net/fabricmc/fabric-installer/1.1.0/fabric-installer-1.1.0.jar";
        let cache_dir = self.config.minecraft_dir.join("cache");
        fs::create_dir_all(&cache_dir).await?;
        let installer_path = cache_dir.join("fabric-installer.jar");

        if !installer_path.exists() {
            let resp = reqwest::get(installer_url).await?;
            let bytes = resp.bytes().await?.to_vec();
            use tokio::io::AsyncWriteExt;
            let mut out = tokio::fs::File::create(&installer_path).await?;
            out.write_all(&bytes).await?;
        }

        // 2. Run Fabric Installer
        // Command: java -jar installer.jar client -dir <mc_dir> -mcversion <ver> -noprofile
        let java_path = self.find_java()?;
        
        let mut command = TokioCommand::new(java_path);
        command
            .arg("-jar")
            .arg(&installer_path)
            .arg("client")
            .arg("-dir")
            .arg(&self.config.minecraft_dir)
            .arg("-mcversion")
            .arg(mc_version)
            .arg("-noprofile") // Do not modify launcher_profiles.json
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = command.output().await?;
        
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Fabric installation failed: {}", err));
        }

        // 3. Find the installed version ID
        // The installer creates a directory in versions/ like "fabric-loader-<loader_ver>-<mc_ver>"
        // We'll search for the most recently modified directory matching this pattern
        let versions_dir = self.config.versions_dir.clone();
        let mut best_match: Option<String> = None;
        let mut latest_time = std::time::SystemTime::UNIX_EPOCH;

        let mut read_dir = tokio::fs::read_dir(&versions_dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.contains("fabric-loader") && name.contains(mc_version) {
                        if let Ok(metadata) = entry.metadata().await {
                            if let Ok(modified) = metadata.modified() {
                                if modified > latest_time {
                                    latest_time = modified;
                                    best_match = Some(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        best_match.ok_or_else(|| anyhow!("Could not find installed Fabric version directory"))
    }
}