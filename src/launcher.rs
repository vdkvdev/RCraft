use anyhow::{anyhow, Result};

use serde::Deserialize;

use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use tokio::fs;
use tokio::process::Command as TokioCommand;

use crate::config::LauncherConfig;
use crate::models::{AssetIndex, Library, MinecraftVersion, VersionManifest};
use crate::utils::{is_library_allowed, is_at_least_1_8};
use crate::java_manager::JavaManager;

#[derive(Clone)]
pub struct MinecraftLauncher {
    pub config: LauncherConfig,
}

#[derive(Deserialize)]
struct VersionJson {
    #[serde(rename = "inheritsFrom")]
    inherits_from: Option<String>,
    #[serde(rename = "javaVersion")]
    java_version: Option<JavaVersion>,
    #[serde(default)]
    libraries: Vec<Library>,
    #[serde(rename = "mainClass")]
    main_class: Option<String>,
    #[serde(rename = "assetIndex")]
    asset_index: Option<AssetIndex>,
}

#[derive(Deserialize)]
struct JavaVersion {
    #[serde(rename = "majorVersion")]
    major_version: u32,
}

impl MinecraftLauncher {
    pub fn new() -> Result<Self> {
        Ok(Self {
            config: LauncherConfig::new()?,
        })
    }

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



    pub async fn get_required_java_version(&self, version: &str) -> Result<u32> {
        let version_dir = self.config.versions_dir.join(version);
        let version_file = version_dir.join(format!("{}.json", version));

        if !version_file.exists() {
             // Fallback heuristic if file not found (e.g. before download?)
             // Should not happen as we download first.
             // But let's assume standard heuristic
             let parts: Vec<&str> = version.split('.').collect();
             if parts.len() >= 2 {
                if let Ok(minor) = parts[1].parse::<u32>() {
                    if minor >= 20 {
                        if parts.len() >= 3 {
                             if let Ok(sub) = parts[2].parse::<u32>() {
                                 if minor == 20 && sub >= 5 {
                                     return Ok(21);
                                 } else if minor > 20 {
                                     return Ok(21);
                                 }
                             }
                        }
                        return Ok(17);
                    } else if minor >= 18 {
                        return Ok(17);
                    } else if minor == 17 {
                        return Ok(16);
                    }
                }
             }
             return Ok(8);
        }

        let version_data = fs::read_to_string(&version_file).await?;
        let version_json: VersionJson = serde_json::from_str(&version_data)?;
        
        // Check java_version field
        if let Some(v) = version_json.java_version {
            return Ok(v.major_version);
        }
        
        if let Some(parent_id) = version_json.inherits_from {
             // Recursive check
             return Box::pin(self.get_required_java_version(&parent_id)).await;
        }

        // Fallback heuristic check on the ID itself if it looks like a vanilla version
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() >= 2 {
            if let Ok(minor) = parts[1].parse::<u32>() {
                if minor >= 20 {
                    if parts.len() >= 3 {
                         if let Ok(sub) = parts[2].parse::<u32>() {
                             if minor == 20 && sub >= 5 {
                                 return Ok(21);
                             } else if minor > 20 {
                                 return Ok(21);
                             }
                         }
                    }
                    return Ok(17);
                } else if minor >= 18 {
                    return Ok(17);
                } else if minor == 17 {
                    return Ok(16);
                }
            }
        }
        
        Ok(8) // Default for older versions without java_version field
    }

    pub async fn prepare_java<F>(&self, version: &str, on_progress: F) -> Result<PathBuf>
    where F: Fn(f64, String) + Send + Sync + 'static + Clone
    {
        let required_version = self.get_required_java_version(version).await?;
        
        // Try to find valid java locally
        if let Ok(path) = self.find_java(Some(required_version)) {
            return Ok(path);
        }
        
        // Not found, download
        let manager = JavaManager::new(self.config.runtimes_dir.clone());
        let path = manager.download_and_install_java(required_version, on_progress).await?;
        
        Ok(path)
    }

    pub async fn build_classpath(&self, start_version: &str) -> Result<String> {
        let mut classpath_paths: Vec<PathBuf> = Vec::new();
        let mut seen_artifacts: std::collections::HashSet<String> = std::collections::HashSet::new(); // group:artifact
        let os_name = "linux";

        let mut current_version_id = Some(start_version.to_string());
        let mut vanilla_jar_path: Option<PathBuf> = None;

        while let Some(version) = current_version_id {
             let version_dir = self.config.versions_dir.join(&version);
             let version_file = version_dir.join(format!("{}.json", version));

             if !version_file.exists() {
                 if version == start_version {
                      return Err(anyhow!("Version JSON not found: {:?}", version_file));
                 } else {
                      break;
                 }
             }

             let version_data = tokio::fs::read_to_string(&version_file).await?;
             let version_json: VersionJson = serde_json::from_str(&version_data)?;

             for lib in &version_json.libraries {
                let allowed = is_library_allowed(lib, os_name);
                if !allowed {
                    continue;
                }

                let mut lib_path_buf: Option<PathBuf> = None;
                let mut maven_key: Option<String> = None;

                if let Some(downloads) = &lib.downloads {
                    if let Some(artifact) = &downloads.artifact {
                        lib_path_buf = Some(self.config.libraries_dir.join(&artifact.path));
                    }
                }

                if lib_path_buf.is_none() {
                     let parts: Vec<&str> = lib.name.split(':').collect();
                     if parts.len() >= 2 {
                         let group = parts[0].replace('.', "/");
                         let artifact_id = parts[1];
                         let version = parts.get(2).unwrap_or(&"");

                         let path = format!("{}/{}/{}/{}-{}.jar", group, artifact_id, version, artifact_id, version);
                         let p = self.config.libraries_dir.join(path);
                         if p.exists() {
                             lib_path_buf = Some(p);
                         }
                     }
                }

                let parts: Vec<&str> = lib.name.split(':').collect();
                if parts.len() >= 2 {
                    let key = format!("{}:{}", parts[0], parts[1]);
                    maven_key = Some(key);
                }

                if let (Some(path), Some(key)) = (lib_path_buf, maven_key) {
                    if !seen_artifacts.contains(&key) {
                        seen_artifacts.insert(key);
                        classpath_paths.push(path);
                    }
                }
             }

             if let Some(parent) = version_json.inherits_from {
                 current_version_id = Some(parent);
             } else {
                 // Base version (Vanilla) -> jar path
                 let jar_path = version_dir.join(format!("{}.jar", version));
                 vanilla_jar_path = Some(jar_path);
                 current_version_id = None;
             }
        }

        if let Some(jar) = vanilla_jar_path {
            classpath_paths.push(jar);
        }

        let cp_string = classpath_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(":");

        Ok(cp_string)
    }

    pub async fn launch_minecraft(&self, version: &str, username: &str, ram_mb: u32, game_dir: &Path) -> Result<TokioCommand> {
        if !is_at_least_1_8(version) {
             return Err(anyhow!("Versions below 1.8 are not supported"));
        }

        let version_dir = self.config.versions_dir.join(version);
        let version_file = version_dir.join(format!("{}.json", version));

        let version_data = fs::read_to_string(&version_file).await?;
        let version_json: VersionJson = serde_json::from_str(&version_data)?;
        
        let required_java = self.get_required_java_version(version).await?;
        let java_path = self.find_java(Some(required_java))?;

        let jar_version = version_json.inherits_from.as_deref().unwrap_or(version);
        let jar_dir = self.config.versions_dir.join(jar_version);
        let jar_path = jar_dir.join(format!("{}.jar", jar_version));

        let natives_version = version_json.inherits_from.as_deref().unwrap_or(version);
        let natives_dir = self.config.versions_dir.join(natives_version).join("natives");

        if !jar_path.exists() {
            return Err(anyhow!("Version JAR not found at: {:?}", jar_path));
        }

        // Check/Repair Natives
        // If natives directory is empty or missing, we must ensure natives are extracted.
        // This is crucial if the version was downloaded before the extraction logic fix was applied.
        let natives_ok = natives_dir.exists() && std::fs::read_dir(&natives_dir).map(|c| c.count() > 0).unwrap_or(false);
        
        if !natives_ok {
            // Re-run library processing for this version to extract natives
             println!("Natives missing for {}, attempting repair...", natives_version);
             let version_file_native = self.config.versions_dir.join(natives_version).join(format!("{}.json", natives_version));
             if version_file_native.exists() {
                 let v_data = fs::read_to_string(&version_file_native).await?;
                 let v_json: VersionJson = serde_json::from_str(&v_data)?;
                 let os_name = "linux"; // we know we are on linux
                 
                 for lib in v_json.libraries {
                     // COPY OF EXTRACTION LOGIC
                    let mut native_artifact = None;
                    if let Some(natives) = &lib.natives {
                         if let Some(classifier) = natives.get(os_name) {
                             if let Some(downloads) = &lib.downloads {
                                 if let Some(classifiers) = &downloads.classifiers {
                                     if let Some(artifact) = classifiers.get(classifier) {
                                         native_artifact = Some(artifact.clone());
                                     }
                                 }
                             }
                         }
                    }
                    if native_artifact.is_none() {
                         if let Some(downloads) = &lib.downloads {
                             if let Some(classifiers) = &downloads.classifiers {
                                 if let Some(artifact) = classifiers.get("natives-linux") {
                                     native_artifact = Some(artifact.clone());
                                 }
                             }
                         }
                    }
                    if native_artifact.is_none() {
                         if let Some(downloads) = &lib.downloads {
                             if let Some(artifact) = &downloads.artifact {
                                 if artifact.path.contains("natives-linux") || lib.name.contains("natives-linux") {
                                     native_artifact = Some(artifact.clone());
                                 }
                             }
                         }
                    }
                    
                    if let Some(artifact) = native_artifact {
                         let native_zip_path = self.config.versions_dir.join(natives_version).join(format!("{}.zip", lib.name.replace(":", "_")));
                         if !native_zip_path.exists() {
                            if let Ok(resp) = reqwest::get(&artifact.url).await {
                                if let Ok(bytes) = resp.bytes().await {
                                     let _ = tokio::fs::write(&native_zip_path, &bytes).await;
                                }
                            }
                         }
                         if native_zip_path.exists() {
                             let nd = natives_dir.clone();
                             let nzp = native_zip_path.clone();
                             let exclude = lib.get_extract().map(|e| e.exclude.clone()).unwrap_or_default();
                             
                             let _ = tokio::task::spawn_blocking(move || {
                                 if let Ok(file) = std::fs::File::open(&nzp) {
                                     if let Ok(mut archive) = zip::ZipArchive::new(file) {
                                          for i in 0..archive.len() {
                                             if let Ok(mut file) = archive.by_index(i) {
                                                  let name = file.name().to_string();
                                                  let excluded = exclude.iter().any(|ex| name.starts_with(ex));
                                                  if excluded || name.ends_with("/") { continue; }
                                                  let filename = std::path::Path::new(&name).file_name().and_then(|f| f.to_str()).unwrap_or(&name).to_string();
                                                  let outpath = nd.join(&filename);
                                                  if let Some(parent) = outpath.parent() { let _ = std::fs::create_dir_all(parent); }
                                                  if let Ok(mut outfile) = std::fs::File::create(&outpath) {
                                                      let _ = std::io::copy(&mut file, &mut outfile);
                                                  }
                                             }
                                          }
                                     }
                                 }
                             }).await;
                         }
                    }
                 }
             }
        }

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
            .arg(main_class)
            .arg("--username")
            .arg(username)
            .arg("--version")
            .arg(version)
            .arg("--gameDir")
            .arg(game_dir)
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
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Ok(command)
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
        let mut candidates = vec![
            "java".to_string(),
            "/usr/bin/java".to_string(),
            "/usr/local/bin/java".to_string(),
            "/opt/java/bin/java".to_string(),
        ];
        
        // Add specific version candidates if requirement is known
        if let Some(ver) = required_version {
            candidates.insert(0, format!("java-{}", ver));
            candidates.insert(0, format!("java{}", ver));
            
            // Common linux paths for specific versions
             candidates.push(format!("/usr/lib/jvm/java-{}-openjdk-amd64/bin/java", ver));
             candidates.push(format!("/usr/lib/jvm/java-{}-openjdk/bin/java", ver));
             // Also try 1.8.0 naming convention for Java 8
             if ver == 8 {
                 candidates.push("/usr/lib/jvm/java-1.8.0-openjdk-amd64/bin/java".to_string());
                 candidates.push("/usr/lib/jvm/java-1.8.0-openjdk/bin/java".to_string());
             }
        } else {
            // Default checks if no version specified (shouldn't happen with new logic but safe fallback)
            candidates.insert(0, "java8".to_string()); 
             candidates.push("/usr/lib/jvm/java-8-openjdk-amd64/bin/java".to_string());
             candidates.push("/usr/lib/jvm/java-8-openjdk-i386/bin/java".to_string());
        }

        // Check environment variable
        if let Ok(java_home) = std::env::var("JAVA_HOME") {
            let path = PathBuf::from(java_home).join("bin").join("java");
             candidates.insert(0, path.display().to_string());
        }

        // Check runtimes directory (managed java)
        let runtime_java = self.config.runtimes_dir.join(format!("java-{}", required_version.unwrap_or(8))).join("bin").join("java");
        if runtime_java.exists() {
             candidates.insert(0, runtime_java.display().to_string());
        }

        let mut found_path: Option<PathBuf> = None;
        let mut found_version: u32 = 0;
        let mut seen_paths = std::collections::HashSet::new();

        // 1. Check specific candidates
        for path_str in candidates {
            let path = if path_str.contains("/") {
                 PathBuf::from(&path_str)
            } else {
                 // Determine path from command
                 if let Ok(output) = StdCommand::new("which").arg(&path_str).output() {
                    if output.status.success() {
                        let p = String::from_utf8(output.stdout).unwrap_or_default().trim().to_string();
                        if p.is_empty() { continue; }
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
                     if seen_paths.contains(&path_abs) { continue; }
                     seen_paths.insert(path_abs.clone());

                     if let Ok(ver) = self.get_java_version(&path_abs) {
                          if let Some(req) = required_version {
                              if ver == req {
                                  return Ok(path_abs);
                              }
                              found_path = Some(path_abs.clone());
                              found_version = ver;
                          } else {
                              return Ok(path_abs);
                          }
                     }
                 }
            }
        }

        // 2. Scan /usr/lib/jvm if required version not found yet
        if let Some(req) = required_version {
            if let Ok(entries) = std::fs::read_dir("/usr/lib/jvm") {
                for entry in entries.flatten() {
                    let path = entry.path().join("bin").join("java");
                    if path.exists() {
                        if let Ok(path_abs) = std::fs::canonicalize(&path) {
                             if seen_paths.contains(&path_abs) { continue; }
                             seen_paths.insert(path_abs.clone());

                             if let Ok(ver) = self.get_java_version(&path_abs) {
                                  if ver == req {
                                      return Ok(path_abs);
                                  }
                                  // Update found path if we haven't found anything yet, or just to track
                                  if found_path.is_none() {
                                      found_path = Some(path_abs);
                                      found_version = ver;
                                  }
                             }
                        }
                    }
                }
            }
        }
        
        if let Some(req) = required_version {
             let msg = if let Some(p) = found_path {
                 format!("Minecraft requires Java {} (found Java {} at {:?}). Please install Java {}.", req, found_version, p, req)
             } else {
                 format!("Minecraft requires Java {} but it was not found on your system. Please install Java {}.", req, req)
             };
             return Err(anyhow!(msg));
        }

        anyhow::bail!("Could not find installed Java")
    }

    #[allow(dead_code)]
    pub fn get_installed_java_versions(&self) -> Vec<String> {
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
        ];

        let mut found_versions = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();

        // Check environment variable
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

        // Check candidates
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

        let java_path = self.find_java(None)?;

        let mut command = TokioCommand::new(java_path);
        command
            .arg("-jar")
            .arg(&installer_path)
            .arg("client")
            .arg("-dir")
            .arg(&self.config.minecraft_dir)
            .arg("-mcversion")
            .arg(mc_version)
            .arg("-noprofile")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = command.output().await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Fabric installation failed: {}", err));
        }

        let versions_dir = self.config.versions_dir.clone();
        let mut best_match: Option<String> = None;
        let mut latest_time = std::time::SystemTime::UNIX_EPOCH;

        let mut read_dir = tokio::fs::read_dir(&versions_dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.contains("fabric-loader") && name.ends_with(&format!("-{}", mc_version)) {
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
