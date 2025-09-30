//  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //
// RCraft - Copyright (C) 2025 @vdkvdev                                             //
//                                                                                  //
// This program is free software under GPL-3.0: key freedoms and restrictions:      //
// - Free use, study, and modification for any purpose.                             //
// - Redistribution only under GPL-3.0 (copyleft: derivatives must be GPL-3).       //
// - Preserve all copyright attributions (including this one).                      //
// - Do not add proprietary clauses or remove notices.                              //
//                                                                                  //
// For the full text, see LICENSE in this repository.                               //
// Repository: https://github.com/vdkvdev/RCraft                                    //
//  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //

use anyhow::Result;

use serde::{Deserialize, Serialize};
use std::env;
use std::io::Read;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

use std::process::{Command, Stdio};
use tokio::fs;
use colored::*;
use dialoguer::{Select, Input, Confirm};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MinecraftVersion {
    id: String,
    #[serde(rename = "type")]
    version_type: String,
    url: String,
    time: String,
    #[serde(rename = "releaseTime")]
    release_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Profile {
    username: String,
    version: String,
    ram_mb: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct VersionManifest {
    versions: Vec<MinecraftVersion>,
}

// --- Structs for libraries and natives ---
#[derive(Deserialize, Debug, Clone)]
struct Extract {
    exclude: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct Rule {
    action: String,
    os: Option<OsRule>,
}

#[derive(Deserialize, Debug, Clone)]
struct OsRule {
    name: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct Library {
    name: String,
    downloads: Option<LibraryDownloads>,
    natives: Option<std::collections::HashMap<String, String>>,
    rules: Option<Vec<Rule>>,
    #[serde(default)]
    extract: Option<Extract>,
}

impl Library {
    fn get_extract(&self) -> Option<&Extract> {
        self.extract.as_ref()
    }
}

#[derive(Deserialize, Debug, Clone)]
struct LibraryDownloads {
    artifact: Option<LibraryArtifact>,
    classifiers: Option<std::collections::HashMap<String, LibraryArtifact>>,
}

#[derive(Deserialize, Debug, Clone)]
struct LibraryArtifact {
    url: String,
    path: String,
}

fn is_library_allowed(lib: &Library, os_name: &str) -> bool {
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

struct LauncherConfig {
    minecraft_dir: PathBuf,
    versions_dir: PathBuf,
    assets_dir: PathBuf,
    libraries_dir: PathBuf,
}

impl LauncherConfig {
    fn new() -> Result<Self> {
        let home = env::var("HOME").unwrap();
        let minecraft_dir = PathBuf::from(home).join(".minecraft");

        Ok(Self {
            versions_dir: minecraft_dir.join("versions"),
            assets_dir: minecraft_dir.join("assets"),
            libraries_dir: minecraft_dir.join("libraries"),
            minecraft_dir,
        })
    }

    async fn ensure_directories(&self) -> Result<()> {
        fs::create_dir_all(&self.minecraft_dir).await?;
        fs::create_dir_all(&self.versions_dir).await?;
        fs::create_dir_all(&self.assets_dir).await?;
        fs::create_dir_all(&self.libraries_dir).await?;
        Ok(())
    }
}

struct MinecraftLauncher {
    config: LauncherConfig,
}

async fn load_profiles(config: &LauncherConfig) -> Result<HashMap<String, Profile>> {
    let path = config.minecraft_dir.join("profiles.json");
    if path.exists() {
        let content = fs::read_to_string(&path).await?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(HashMap::new())
    }
}

async fn save_profiles(config: &LauncherConfig, profiles: &HashMap<String, Profile>) -> Result<()> {
    let path = config.minecraft_dir.join("profiles.json");
    let content = serde_json::to_string_pretty(profiles)?;
    fs::write(&path, content).await?;
    Ok(())
}

fn parse_version(s: &str) -> (i32, i32, i32) {
    let parts: Vec<&str> = s.split('.').collect();
    (
        parts.get(0).unwrap_or(&"0").parse().unwrap_or(0),
        parts.get(1).map_or(0, |x| x.parse().unwrap_or(0)),
        parts.get(2).map_or(0, |x| x.parse().unwrap_or(0)),
    )
}

fn compare_versions(a: &str, b: &str) -> Ordering {
    let pa = parse_version(a);
    let pb = parse_version(b);
    (pa.0, pa.1, pa.2).cmp(&(pb.0, pb.1, pb.2))
}

fn is_at_least_1_8(v: &str) -> bool {
    let p = parse_version(v);
    p.0 > 1 || (p.0 == 1 && p.1 >= 8)
}

impl MinecraftLauncher {
    fn new() -> Result<Self> {
        Ok(Self {
            config: LauncherConfig::new()?,
        })
    }

    async fn get_available_versions(&self) -> Result<Vec<MinecraftVersion>> {
        let url = "https://launchermeta.mojang.com/mc/game/version_manifest.json";
        let response = reqwest::get(url).await?;
        let manifest: VersionManifest = response.json().await?;

        // Filter only release versions
        let release_versions: Vec<MinecraftVersion> = manifest
            .versions
            .into_iter()
            .filter(|v| v.version_type == "release")
            .collect();

        Ok(release_versions)
    }

    async fn download_version(&self, version: &MinecraftVersion) -> Result<()> {
        println!("{}", "Downloading version...".green());

        let version_dir = self.config.versions_dir.join(&version.id);
        fs::create_dir_all(&version_dir).await?;
        let natives_dir = version_dir.join("natives");
        fs::create_dir_all(&natives_dir).await?;



        // Download the version file
        let version_response = reqwest::get(&version.url).await?;
        let version_data = version_response.text().await?;

        // Save the version file
        let version_file = version_dir.join(format!("{}.json", version.id));
        fs::write(&version_file, &version_data).await?;

        // --- Download client.jar ---
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
        #[derive(Deserialize)]
        struct AssetIndex {
            id: String,
            url: String,
        }
        #[derive(Deserialize)]
        struct AssetIndexJson {
            objects: std::collections::HashMap<String, AssetObject>,
        }
        #[derive(Deserialize)]
        struct AssetObject {
            hash: String,
        }
        let version_json: VersionJson = serde_json::from_str(&version_data)?;
        let jar_url = if let Some(downloads) = &version_json.downloads {
            downloads.client.as_ref().map(|c| c.url.clone())
        } else {
            None
        }.unwrap_or_else(|| format!("https://s3.amazonaws.com/Minecraft.Download/versions/{}/{}.jar", &version.id, &version.id));

        let jar_path = version_dir.join(format!("{}.jar", version.id));

        let resp = reqwest::get(&jar_url).await?;
        let bytes = resp.bytes().await?.to_vec();
        let mut out = tokio::fs::File::create(&jar_path).await?;
        use tokio::io::AsyncWriteExt;
        out.write_all(&bytes).await?;
        // --- Download libraries and natives ---
        // progress.set_message("Downloading libraries and natives...");
        let os_name = "linux";
        // Check if natives already exist to skip extraction
        let has_natives = std::fs::read_dir(&natives_dir).is_ok_and(|rd| rd.count() > 0);
        if !has_natives {
            // Clean natives folder before extracting
            if natives_dir.exists() {
                std::fs::remove_dir_all(&natives_dir)?;
            }
            std::fs::create_dir_all(&natives_dir)?;
        }
        for lib in &version_json.libraries {
            let allowed = is_library_allowed(lib, os_name);
            if !allowed {
                continue;
            }
            // Download normal library
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
                        use tokio::io::AsyncWriteExt;
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
                                    use tokio::io::AsyncWriteExt;
                                    out.write_all(&bytes).await?;
                                    out.flush().await?;
                                    out.sync_all().await?;
                                }
                                if has_natives {
                                    continue;
                                }
                                // Extract natives respecting extract.exclude
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
                                        // Exclude folders and files based on extract.exclude
                                        let excluded = exclude.iter().any(|ex| name.starts_with(ex));
                                        if excluded || name.ends_with("/") {
                                            continue;
                                        }
                                        let filename = std::path::Path::new(&name).file_name().and_then(|f| f.to_str()).unwrap_or(&name).to_string();
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
                                    eprintln!("Warning: Failed to extract natives for {}: {}, continuing...", lib.name, e);
                                }
                            }
                        }
                    }
                }
            }
        }
        // List extracted natives for debugging
        // let natives_files = std::fs::read_dir(&natives_dir)?
        //     .filter_map(|e| e.ok())
        //     .map(|e| e.file_name().to_string_lossy().into_owned())
        //     .collect::<Vec<_>>();
        // println!("Natives extracted: {:?}", natives_files);
        // progress.finish_with_message("Version, JAR, libraries and natives downloaded successfully");
        // Download assets
        if let Some(asset_index) = &version_json.asset_index {
            let indexes_dir = self.config.assets_dir.join("indexes");
            fs::create_dir_all(&indexes_dir).await?;
            let index_path = indexes_dir.join(format!("{}.json", asset_index.id));

            let resp = reqwest::get(&asset_index.url).await?;
            let bytes = resp.bytes().await?.to_vec();
            let mut out = tokio::fs::File::create(&index_path).await?;
            use tokio::io::AsyncWriteExt;
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
                    use tokio::io::AsyncWriteExt;
                    out.write_all(&bytes).await?;
                }
            }
        }
        // Post-extraction check
        //if std::fs::read_dir(&natives_dir).is_ok_and(|rd| rd.count() == 0) {
        //    eprintln!("Warning: No natives extracted to {}, may cause launch issues", natives_dir.display());
        //}
        Ok(())
    }

    async fn build_classpath(&self, version: &str) -> Result<String> {
        let version_dir = self.config.versions_dir.join(version);
        let version_file = version_dir.join(format!("{}.json", version));
let version_data = tokio::fs::read_to_string(&version_file).await?;
        #[derive(Deserialize)]
        struct VersionJson {
            libraries: Vec<Library>,
        }
        let version_json: VersionJson = serde_json::from_str(&version_data)?;
        let os_name = "linux";
        let mut classpath = Vec::new();
        for lib in &version_json.libraries {
            let allowed = is_library_allowed(lib, os_name);
            if !allowed {
                continue;
            }
            if let Some(downloads) = &lib.downloads {
                if let Some(artifact) = &downloads.artifact {
                    let lib_path = self.config.libraries_dir.join(&artifact.path);
                    classpath.push(lib_path);
                }
            }
        }
        // Add the main JAR at the end
        let jar_path = version_dir.join(format!("{}.jar", version));
        classpath.push(jar_path);
        // Join using ':' (for Linux)
        let cp = classpath.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(":");
        Ok(cp)
    }

    async fn launch_minecraft(
        &self,
        version: &str,
        username: &str,
        ram_mb: u32,
    ) -> Result<()> {
        #[derive(Deserialize)]
        struct VersionJson {
            #[serde(rename = "assetIndex")]
            asset_index: Option<AssetIndex>,
        }
        #[derive(Deserialize)]
        struct AssetIndex {
            id: String,
        }
        println!("{}", "Launching Minecraft...".blue());

        // Check version support
        let version_parts: Vec<&str> = version.split('.').collect();
        if version_parts.len() >= 2 && version_parts[0] == "1" {
            if let Ok(minor) = version_parts[1].parse::<u32>() {
                if minor < 8 {
                    println!("{}", "Versions below 1.8 are not supported. Please use 1.8 or higher.".red());
                    return Ok(());
                }
            }
        }

        let java_path = self.find_java()?;
        let version_dir = self.config.versions_dir.join(version);
        let jar_path = version_dir.join(format!("{}.jar", version));
        let natives_dir = version_dir.join("natives");

        if !jar_path.exists() {
            println!("{}", "Error: Version not downloaded".red());
            return Ok(());
        }
        // Build full classpath
        let classpath = self.build_classpath(version).await?;
        let mut command = Command::new(java_path);
        command
            .arg("-Xmx".to_string() + &ram_mb.to_string() + "M")
            .arg("-Xms".to_string() + &(ram_mb / 2).to_string() + "M")
            .arg("-Djava.library.path=".to_string() + &natives_dir.display().to_string())
            .arg("-cp")
            .arg(classpath)
            .arg("net.minecraft.client.main.Main")
            .arg("--username")
            .arg(username)
            .arg("--version")
            .arg(version)
            .arg("--gameDir")
            .arg(&self.config.minecraft_dir)
            .arg("--assetsDir")
            .arg(&self.config.assets_dir);

        let version_file = version_dir.join(format!("{}.json", version));
        let version_data = fs::read_to_string(&version_file).await?;
        let version_json: VersionJson = serde_json::from_str(&version_data)?;

        if let Some(asset_index) = version_json.asset_index {
            command
                .arg("--assetIndex")
                .arg(asset_index.id);
        }
            command.arg("--accessToken")
            .arg("0")
            .arg("--userProperties")
            .arg("{}")
            .current_dir(&version_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = command.spawn()?;

        let status = child.wait()?;

        if !status.success() {
            let mut err = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut err);
            }
            println!("{}", "Error running Minecraft".red());
            if !err.is_empty() {
                println!("{} {}", "Error details:".red(), err);
            }
            return Ok(());
        }

        Ok(())
    }

    fn find_java(&self) -> Result<PathBuf> {
    // Search for Java8 first
    if let Ok(output) = Command::new("which").arg("java8").output() {
        if output.status.success() {
            let java_path = String::from_utf8(output.stdout)?;
            return Ok(PathBuf::from(java_path.trim()));
        }
    }
    // Search for Java
    if let Ok(output) = Command::new("which").arg("java").output() {
        if output.status.success() {
            let java_path = String::from_utf8(output.stdout)?;
            return Ok(PathBuf::from(java_path.trim()));
        }
    }
    // Search in common locations
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
    }
fn get_total_ram_mb() -> Result<u32> {
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
    anyhow::bail!("Could not find MemTotal in /proc/meminfo");
}
#[tokio::main]
async fn main() -> Result<()> {
    let banner = r#"
 ██████╗  ██████╗██████╗  █████╗ ███████╗████████╗
 ██╔══██╗██╔════╝██╔══██╗██╔══██╗██╔════╝╚══██╔══╝
 ██████╔╝██║     ██████╔╝███████║█████╗     ██║
 ██╔══██╗██║     ██╔══██╗██╔══██║██╔══╝     ██║
 ██║  ██║╚██████╗██║  ██║██║  ██║██║        ██║
 ╚═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝        ╚═╝
              v0.5 - by @vdkvdev
"#.yellow();
    println!("{banner}");

    let launcher = MinecraftLauncher::new()?;

    // Ensure directories exist
    launcher.config.ensure_directories().await?;

    let mut profiles = load_profiles(&launcher.config).await?;
    let versions = launcher.get_available_versions().await?;
    let filtered_versions: Vec<_> = versions.iter().filter(|v| is_at_least_1_8(&v.id)).cloned().collect();
    let mut sorted_versions = filtered_versions.clone();
    sorted_versions.sort_by(|a, b| compare_versions(&a.id, &b.id));

    loop {
        let create_item = format!("{}", "[+] Create Profile".blue());
        let exit_item = format!("{}", "Exit".red());
        let mut display_items = vec![create_item];
        let profile_keys: Vec<String> = profiles.keys().cloned().collect();
        let profile_displays: Vec<String> = profile_keys.iter().map(|name| {
            let p = profiles.get(name).unwrap();
            format!("{} (v{}, {}MB)", format!("{}", name.green()), p.version, p.ram_mb)
        }).collect();
        display_items.extend(profile_displays);
        display_items.push(exit_item);
        let selection = Select::new()
            .with_prompt("Choose an option:")
            .items(&display_items)
            .interact()?;

        if selection == 0 {
            // Create
            let username: String = Input::new().with_prompt("Username (min 3, max 16 chars)").interact()?;
            if username.is_empty() || username.len() < 3 || username.len() > 16 || profiles.contains_key(&username) {
                println!("{}", "Invalid: min 3, max 16 chars or already exists!".red());
                continue;
            }
            let min_version = sorted_versions[0].id.clone();
            let max_version = sorted_versions.last().unwrap().id.clone();
            let version;
            loop {
                let candidate: String = Input::new().with_prompt(format!("Minecraft version (min {}, max {})", min_version, max_version)).interact()?;
                if is_at_least_1_8(&candidate) && sorted_versions.iter().any(|v| v.id == candidate) {
                    version = candidate;
                    break;
                }
                println!("{}", format!("Invalid version. Use between {} and {}", min_version, max_version).red());
            }
            let available_mb = get_total_ram_mb()? as u64;
            let ram_str: String = Input::new().with_prompt(format!("RAM in MB (min 1024, max {})", available_mb)).interact()?;
            if let Ok(ram) = ram_str.parse::<u32>() {
                if ram >= 1024 && (ram as u64) <= available_mb {
                    profiles.insert(username.clone(), Profile { username: username.clone(), version, ram_mb: ram });
                    save_profiles(&launcher.config, &profiles).await?;
                    println!("{}", "Profile created!".green());
                } else {
                    println!("{}", "Invalid RAM".red());
                }
            } else {
                println!("{}", "Invalid RAM".red());
            }
        } else if selection == display_items.len() - 1 {
            break;
        } else {
            let profile_idx = selection - 1;
            let profile_name = profile_keys[profile_idx].clone();
            let action_items = vec![format!("{}", "Launch".green()), format!("{}", "Delete".red())];
            let action_sel = Select::new()
                .with_prompt(format!("What do you want to do with {}?", profile_name.green()))
                .items(&action_items)
                .interact()?;
            if action_sel == 0 {
                // Launch
                let profile = profiles.get(&profile_name).unwrap().clone();
                println!("{}", format!("Launching {}...", profile_name.green()));
                let target_version = sorted_versions.iter().find(|v| v.id == profile.version).unwrap();
                let available_mb = get_total_ram_mb()? as u64;
                let ram_mb = std::cmp::min(profile.ram_mb as u64, available_mb) as u32;
                // Check download
                let version_dir = launcher.config.versions_dir.join(&profile.version);
                let jar_path = version_dir.join(format!("{}.jar", profile.version));
                let natives_dir = version_dir.join("natives");
                let jopt_simple_path = launcher.config.libraries_dir.join("net/sf/jopt-simple/jopt-simple/4.6/jopt-simple-4.6.jar");
                let natives_exist = if !natives_dir.exists() {
                    false
                } else {
                    match tokio::fs::read_dir(&natives_dir).await {
                        Ok(mut d) => match d.next_entry().await {
                            Ok(Some(_)) => true,
                            _ => false,
                        },
                        Err(_) => false,
                    }
                };
                let need_download = !jar_path.exists() || !jopt_simple_path.exists() || !natives_exist;
                if need_download {
                    launcher.download_version(target_version).await?;
                }
                launcher.launch_minecraft(&profile.version, &profile.username, ram_mb).await?;
                break;
            } else {
                // Delete
                if Confirm::new().with_prompt(format!("Delete {}?", profile_name.red())).interact()? {
                    profiles.remove(&profile_name);
                    save_profiles(&launcher.config, &profiles).await?;
                    println!("{}", "Deleted!".green());
                }
            }
        }
    }

    Ok(())
}
