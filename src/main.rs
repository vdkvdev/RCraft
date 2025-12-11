//  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //  //
// RCraft v0.6                                                                      //
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
use std::cmp::Ordering;
use std::collections::HashMap;
use std::env;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tokio::fs;

use iced::widget::{button, column, container, pick_list, row, scrollable, text, text_input, Column, Space, slider, mouse_area};
use iced::window;
use iced::{Element, Length, Task, Size};

// ============================================================================
// Data Structures
// ============================================================================

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
    #[serde(default)]
    playtime_seconds: u64,
    #[serde(default)]
    last_launch: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VersionManifest {
    versions: Vec<MinecraftVersion>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Section {
    Home,
    CreateInstance,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum Language {
    English,
    Spanish,
}

impl Default for Language {
    fn default() -> Self {
        Language::English
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Language::English => write!(f, "English"),
            Language::Spanish => write!(f, "Español"),
        }
    }
}


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
    natives: Option<HashMap<String, String>>,
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
    classifiers: Option<HashMap<String, LibraryArtifact>>,
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

        let release_versions: Vec<MinecraftVersion> = manifest
            .versions
            .into_iter()
            .filter(|v| v.version_type == "release")
            .collect();

        Ok(release_versions)
    }

    async fn download_version(&self, version: &MinecraftVersion) -> Result<()> {
        let version_dir = self.config.versions_dir.join(&version.id);
        fs::create_dir_all(&version_dir).await?;
        let natives_dir = version_dir.join("natives");
        fs::create_dir_all(&natives_dir).await?;

        let version_response = reqwest::get(&version.url).await?;
        let version_data = version_response.text().await?;
        let version_file = version_dir.join(format!("{}.json", version.id));
        fs::write(&version_file, &version_data).await?;

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
            objects: HashMap<String, AssetObject>,
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
        }
        .unwrap_or_else(|| {
            format!(
                "https://s3.amazonaws.com/Minecraft.Download/versions/{}/{}.jar",
                &version.id, &version.id
            )
        });

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

        let jar_path = version_dir.join(format!("{}.jar", version));
        classpath.push(jar_path);
        let cp = classpath
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(":");
        Ok(cp)
    }

    async fn launch_minecraft(&self, version: &str, username: &str, ram_mb: u32) -> Result<()> {
        #[derive(Deserialize)]
        struct VersionJson {
            #[serde(rename = "assetIndex")]
            asset_index: Option<AssetIndex>,
        }
        #[derive(Deserialize)]
        struct AssetIndex {
            id: String,
        }

        let version_parts: Vec<&str> = version.split('.').collect();
        if version_parts.len() >= 2 && version_parts[0] == "1" {
            if let Ok(minor) = version_parts[1].parse::<u32>() {
                if minor < 8 {
                    return Err(anyhow::anyhow!("Versions below 1.8 are not supported"));
                }
            }
        }

        let java_path = self.find_java()?;
        let version_dir = self.config.versions_dir.join(version);
        let jar_path = version_dir.join(format!("{}.jar", version));
        let natives_dir = version_dir.join("natives");

        if !jar_path.exists() {
            return Err(anyhow::anyhow!("Version not downloaded"));
        }

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
            command.arg("--assetIndex").arg(asset_index.id);
        }

        command
            .arg("--accessToken")
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
            return Err(anyhow::anyhow!("Minecraft launch failed: {}", err));
        }

        Ok(())
    }

    fn find_java(&self) -> Result<PathBuf> {
        if let Ok(output) = Command::new("which").arg("java8").output() {
            if output.status.success() {
                let java_path = String::from_utf8(output.stdout)?;
                return Ok(PathBuf::from(java_path.trim()));
            }
        }
        if let Ok(output) = Command::new("which").arg("java").output() {
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

// ============================================================================
// GUI
// ============================================================================

#[derive(Debug, Clone)]
enum Message {
    // Main menu
    ProfileSelected(String),
    CreateNewProfile,
    LaunchProfile(String),
    DeleteProfile(String),

    // Create profile
    UsernameChanged(String),
    VersionSelected(String),
    RamChanged(u32),
    SaveProfile,
    CancelCreate,

    // Async operations
    VersionsLoaded(Result<Vec<MinecraftVersion>, String>),
    ProfilesLoaded(Result<HashMap<String, Profile>, String>),
    DownloadCompleted(Result<(), String>),
    LaunchCompleted(Result<(), String>),

    // Navigation
    NavigateToSection(Section),
    BackToMainMenu,
    UpdateDownloadDots,
    OpenSettings,
    OpenMinecraftFolder,
    LanguageSelected(Language),

    // Window controls
    MinimizeWindow,
    CloseWindow,
    DragWindow,
}

#[derive(Debug, Clone)]
enum AppState {
    Loading,
    Ready { current_section: Section },
    Downloading { profile_name: String },
    Launching { profile_name: String },
    Error { message: String },
}

struct RCraftApp {
    state: AppState,
    launcher: Option<MinecraftLauncher>,
    profiles: HashMap<String, Profile>,
    available_versions: Vec<MinecraftVersion>,
    sorted_versions: Vec<String>,

    // Create profile form
    input_username: String,
    input_version: Option<String>,
    input_ram: u32,

    error_message: Option<String>,

    // Download animation
    download_dots: u8,

    // Language
    language: Language,
}

impl Default for RCraftApp {
    fn default() -> Self {
        Self {
            state: AppState::Loading,
            launcher: MinecraftLauncher::new().ok(),
            profiles: HashMap::new(),
            available_versions: Vec::new(),
            sorted_versions: Vec::new(),
            input_username: String::new(),
            input_version: None,
            input_ram: 4096,
            error_message: None,
            download_dots: 1,
            language: Language::default(),
        }
    }
}

impl RCraftApp {
    fn new() -> (Self, Task<Message>) {
        let app = Self::default();

        let task = Task::future(async {
            let launcher = MinecraftLauncher::new().unwrap();
            launcher.config.ensure_directories().await.ok();

            let versions_result = launcher.get_available_versions().await
                .map_err(|e| e.to_string());

            Message::VersionsLoaded(versions_result)
        });

        (app, task)
    }

    fn title(&self) -> String {
        String::from("RCraft v0.6 - Minecraft Launcher")
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::VersionsLoaded(result) => {
                match result {
                    Ok(versions) => {
                        let mut filtered: Vec<_> = versions.into_iter()
                            .filter(|v| is_at_least_1_8(&v.id))
                            .collect();
                        filtered.sort_by(|a, b| compare_versions(&b.id, &a.id));

                        self.sorted_versions = filtered.iter().map(|v| v.id.clone()).collect();
                        self.available_versions = filtered;

                        // Load profiles
                        if let Some(launcher) = &self.launcher {
                            let config = launcher.config.minecraft_dir.clone();
                            return Task::future(async move {
                                let path = config.join("profiles.json");
                                let profiles = if path.exists() {
                                    let content = tokio::fs::read_to_string(&path).await.ok().unwrap_or_default();
                                    serde_json::from_str(&content).unwrap_or_default()
                                } else {
                                    HashMap::new()
                                };
                                Message::ProfilesLoaded(Ok(profiles))
                            });
                        }
                    }
                    Err(e) => {
                        self.state = AppState::Error { message: format!("Failed to load versions: {}", e) };
                    }
                }
            }
            Message::ProfilesLoaded(result) => {
                match result {
                    Ok(profiles) => {
                        self.profiles = profiles;
                        self.state = AppState::Ready { current_section: Section::Home };

                        // Load saved language
                        if let Some(launcher) = &self.launcher {
                            let config_dir = launcher.config.minecraft_dir.clone();
                            let lang_path = config_dir.join("language.json");
                            if lang_path.exists() {
                                if let Ok(content) = std::fs::read_to_string(&lang_path) {
                                    if let Ok(lang) = serde_json::from_str::<Language>(&content) {
                                        self.language = lang;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        self.error_message = Some(format!("Failed to load profiles: {}", e));
                        self.state = AppState::Ready { current_section: Section::Home };
                    }
                }
            }
            Message::CreateNewProfile => {
                // This message is now handled by NavigateToSection
                self.input_username.clear();
                self.input_version = None;
                self.input_ram = 4096;
                self.error_message = None;
            }
            Message::NavigateToSection(section) => {
                if let AppState::Ready { .. } = self.state {
                    self.state = AppState::Ready { current_section: section };
                    // Clear form when navigating to CreateInstance
                    if section == Section::CreateInstance {
                        self.input_username.clear();
                        self.input_version = None;
                        self.input_ram = 4096;
                        self.error_message = None;
                    }
                }
            }
            Message::UsernameChanged(value) => {
                self.input_username = value;
            }
            Message::VersionSelected(version) => {
                self.input_version = Some(version);
            }
            Message::RamChanged(value) => {
                self.input_ram = value;
            }
            Message::SaveProfile => {
                // Validation
                if self.input_username.len() < 3 || self.input_username.len() > 16 {
                    self.error_message = Some("Username must be 3-16 characters".to_string());
                    return Task::none();
                }

                if self.profiles.contains_key(&self.input_username) {
                    self.error_message = Some("Username already exists".to_string());
                    return Task::none();
                }

                let version = match &self.input_version {
                    Some(v) => v.clone(),
                    None => {
                        self.error_message = Some("Please select a version".to_string());
                        return Task::none();
                    }
                };

                let ram = self.input_ram;

                let profile = Profile {
                    username: self.input_username.clone(),
                    version: version.clone(),
                    ram_mb: ram,
                    playtime_seconds: 0,
                    last_launch: None,
                };

                self.profiles.insert(self.input_username.clone(), profile);

                // Save profiles
                if let Some(launcher) = &self.launcher {
                    let config_dir = launcher.config.minecraft_dir.clone();
                    let profiles = self.profiles.clone();

                    tokio::spawn(async move {
                        let _ = tokio::fs::create_dir_all(&config_dir).await;
                        let path = config_dir.join("profiles.json");
                        let content = serde_json::to_string_pretty(&profiles).unwrap();
                        let _ = tokio::fs::write(&path, content).await;
                    });
                }

                self.state = AppState::Ready { current_section: Section::Home };
                self.error_message = None;
            }
            Message::CancelCreate => {
                // Navigation now handled by sidebar, just clear error
                self.error_message = None;
            }
            Message::LaunchProfile(profile_name) => {
                if let Some(mut profile) = self.profiles.get(&profile_name).cloned() {
                    if let Some(_launcher) = &self.launcher {
                        let version_obj = self.available_versions.iter()
                            .find(|v| v.id == profile.version)
                            .cloned();

                        if let Some(version) = version_obj {
                            // Record launch start time
                            let launch_start = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            profile.last_launch = Some(launch_start);
                            self.profiles.insert(profile_name.clone(), profile.clone());

                            // Save profiles with launch time
                            if let Some(launcher) = &self.launcher {
                                let config_dir = launcher.config.minecraft_dir.clone();
                                let profiles = self.profiles.clone();
                                tokio::spawn(async move {
                                    let _ = tokio::fs::create_dir_all(&config_dir).await;
                                    let path = config_dir.join("profiles.json");
                                    let content = serde_json::to_string_pretty(&profiles).unwrap();
                                    let _ = tokio::fs::write(&path, content).await;
                                });
                            }

                            // Clear dots animation and start downloading
                            self.download_dots = 1;
                            self.state = AppState::Downloading { profile_name: profile_name.clone() };

                            let profile_clone = profile.clone();
                            let config_dir = self.launcher.as_ref().unwrap().config.minecraft_dir.clone();
                            let all_profiles = self.profiles.clone();

                            let download_task = Task::future(async move {
                                let launcher_clone = MinecraftLauncher::new().unwrap();
                                // Check if download needed
                                let version_dir = launcher_clone.config.versions_dir.join(&profile_clone.version);
                                let jar_path = version_dir.join(format!("{}.jar", profile_clone.version));

                                if !jar_path.exists() {
                                    match launcher_clone.download_version(&version).await {
                                        Ok(_) => {},
                                        Err(e) => return Message::DownloadCompleted(Err(e.to_string())),
                                    }
                                }

                                // Launch and track time
                                let launch_start = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs();

                                match launcher_clone.launch_minecraft(&profile_clone.version, &profile_clone.username, profile_clone.ram_mb).await {
                                    Ok(_) => {
                                        // Calculate playtime
                                        let launch_end = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs();
                                        let session_time = launch_end - launch_start;

                                        // Update playtime
                                        let mut updated_profiles = all_profiles;
                                        if let Some(p) = updated_profiles.get_mut(&profile_clone.username) {
                                            p.playtime_seconds += session_time;
                                        }

                                        // Save updated playtime
                                        let _ = tokio::fs::create_dir_all(&config_dir).await;
                                        let path = config_dir.join("profiles.json");
                                        let content = serde_json::to_string_pretty(&updated_profiles).unwrap();
                                        let _ = tokio::fs::write(&path, content).await;

                                        Message::LaunchCompleted(Ok(()))
                                    },
                                    Err(e) => Message::LaunchCompleted(Err(e.to_string())),
                                }
                            });

                            // Start animation task
                            let animation_task = Task::future(async {
                                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                Message::UpdateDownloadDots
                            });

                            return Task::batch(vec![download_task, animation_task]);
                        }
                    }
                }
            }
           Message::DownloadCompleted(result) => {
                match result {
                    Ok(_) => {
                        self.state = AppState::Ready { current_section: Section::Home };
                    }
                    Err(e) => {
                        self.state = AppState::Error { message: format!("Download/Launch failed: {}", e) };
                    }
                }
            }
            Message::LaunchCompleted(result) => {
                match result {
                    Ok(_) => {
                        self.state = AppState::Ready { current_section: Section::Home };
                    }
                    Err(e) => {
                        self.state = AppState::Error { message: format!("Launch failed: {}", e) };
                    }
                }
            }
            Message::DeleteProfile(profile_name) => {
                self.profiles.remove(&profile_name);

                // Save profiles
                if let Some(launcher) = &self.launcher {
                    let config_dir = launcher.config.minecraft_dir.clone();
                    let profiles = self.profiles.clone();

                    tokio::spawn(async move {
                        let _ = tokio::fs::create_dir_all(&config_dir).await;
                        let path = config_dir.join("profiles.json");
                        let content = serde_json::to_string_pretty(&profiles).unwrap();
                        let _ = tokio::fs::write(&path, content).await;
                    });
                }
            }
            Message::BackToMainMenu => {
                self.state = AppState::Ready { current_section: Section::Home };
                self.error_message = None;
            }
            Message::MinimizeWindow => {
                return window::get_oldest().then(|id_opt| {
                    if let Some(id) = id_opt {
                        window::minimize(id, true)
                    } else {
                        Task::none()
                    }
                });
            }
            Message::CloseWindow => {
                return window::get_oldest().then(|id_opt| {
                    if let Some(id) = id_opt {
                        window::close(id)
                    } else {
                        Task::none()
                    }
                });
            }
            Message::DragWindow => {
                return window::get_oldest().then(|id_opt| {
                    if let Some(id) = id_opt {
                        window::drag(id)
                    } else {
                        Task::none()
                    }
                });
            }
            Message::UpdateDownloadDots => {
                // Cycle dots: 1 -> 2 -> 3 -> 1
                self.download_dots = if self.download_dots >= 3 { 1 } else { self.download_dots + 1 };

                // Continue animation if still downloading
                if matches!(self.state, AppState::Downloading { .. }) {
                    return Task::future(async {
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        Message::UpdateDownloadDots
                    });
                }
            }
            Message::OpenSettings => {
                // Navigation now handled by sidebar
            }
            Message::OpenMinecraftFolder => {
                if let Some(launcher) = &self.launcher {
                    let minecraft_dir = launcher.config.minecraft_dir.clone();
                    tokio::spawn(async move {
                        let _ = Command::new("xdg-open")
                            .arg(&minecraft_dir)
                            .spawn();
                    });
                }
            }
            Message::LanguageSelected(lang) => {
                self.language = lang;

                // Save language preference
                if let Some(launcher) = &self.launcher {
                    let config_dir = launcher.config.minecraft_dir.clone();
                    tokio::spawn(async move {
                        let lang_path = config_dir.join("language.json");
                        let content = serde_json::to_string(&lang).unwrap_or_default();
                        let _ = tokio::fs::write(&lang_path, content).await;
                    });
                }
            }
            _ => {}
        }

        Task::none()
    }

    fn view(&self) -> Element<Message> {
        let titlebar = self.view_titlebar();

        let view_content: Element<Message> = match &self.state {
            AppState::Loading => {
                container(
                    text(self.t("loading")).size(16).color(iced::Color::from_rgb8(148, 163, 184))
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::Alignment::Center)
                .align_y(iced::Alignment::Center)
                        .into()

            }
            AppState::Ready { current_section } => {
                let sidebar = self.view_sidebar(*current_section);
                let main_content = match current_section {
                    Section::Home => self.view_main_menu(),
                    Section::CreateInstance => self.view_create_profile(),
                    Section::Settings => self.view_settings(),
                };

                let content_area = container(main_content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding([0, 20]);

                row![
                    sidebar,
                    content_area
                ].into()
            }
            AppState::Downloading { profile_name: _ } => {
                // Generate dots based on download_dots field (1, 2, or 3)
                let dots = match self.download_dots {
                    1 => ".",
                    2 => "..",
                    _ => "...",
                };

                let content = column![
                    text(format!("{}{}", self.t("launching"), dots))
                        .size(20)
                        .color(iced::Color::from_rgb8(255, 255, 255)),
                    Space::with_height(10),
                    text(self.t("first_launch_msg"))
                        .size(12)
                        .color(iced::Color::from_rgb8(148, 163, 184)),
                ]
                .spacing(5)
                .align_x(iced::Alignment::Center);

                // Wrap in centered container
                container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(iced::Alignment::Center)
                    .align_y(iced::Alignment::Center)
                        .into()
            }
            AppState::Launching { profile_name } => {
                column![
                    text(format!("Launching {}...", profile_name)).size(20).color(iced::Color::from_rgb8(16, 185, 129))
                ]
                .spacing(15)
                        .into()
            }
            AppState::Error { message } => {
                column![
                    text("Error").size(28).color(iced::Color::from_rgb8(239, 68, 68)),
                    Space::with_height(10),
                    text(message).size(14).color(iced::Color::from_rgb8(226, 232, 240)),
                    Space::with_height(10),
                    button(text("Back").size(14)).on_press(Message::BackToMainMenu).padding([8, 16])
                        .style(|_theme, _status| {
                            button::Style {
                                background: Some(iced::Background::Color(iced::Color::from_rgb8(59, 130, 246))),
                                text_color: iced::Color::from_rgb8(226, 232, 240),
                                border: iced::Border::default(),
                                ..Default::default()
                            }
                        }),
                ]
                .spacing(15)
                .align_x(iced::Alignment::Center).into()
            }
        };

        let main_content = container(view_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(20)
            .style(|_theme| {
                container::Style {
                    background: Some(iced::Background::Color(iced::Color::from_rgb8(10, 14, 26))),
                    ..Default::default()
                }
            });

        column![titlebar, main_content].into()
    }

    fn view_titlebar(&self) -> Element<Message> {
        let title = text("RCraft v0.6").size(13).color(iced::Color::from_rgb8(59, 130, 246));

        let minimize_btn = button(text("-").size(16))
            .on_press(Message::MinimizeWindow)
            .padding([2, 10])
            .style(|_theme, _status| {
                button::Style {
                    background: Some(iced::Background::Color(iced::Color::TRANSPARENT)),
                    text_color: iced::Color::from_rgb8(148, 163, 184),
                    border: iced::Border::default(),
                    ..Default::default()
                }
            });

        let close_btn = button(text("×").size(20))
            .on_press(Message::CloseWindow)
            .padding([0, 10])
            .style(|_theme, _status| {
                button::Style {
                    background: Some(iced::Background::Color(iced::Color::TRANSPARENT)),
                    text_color: iced::Color::from_rgb8(239, 68, 68),
                    border: iced::Border::default(),
                    ..Default::default()
                }
            });

        let titlebar_content = container(
            row![
                title,
                Space::with_width(Length::Fill),
                minimize_btn,
                close_btn,
            ]
            .align_y(iced::Alignment::Center)
        )
        .width(Length::Fill)
        .padding([8, 15])
        .style(|_theme| {
            container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgb8(10, 14, 26))),
                border: iced::Border {
                    color: iced::Color::from_rgb8(45, 54, 84),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        });

        // Make titlebar draggable
        mouse_area(titlebar_content)
            .on_press(Message::DragWindow)
            .into()
    }

    fn view_sidebar(&self, current_section: Section) -> Element<Message> {
        let home_btn = button(
            text(self.t("home")).size(14)
        )
        .width(Length::Fill)
        .padding([12, 16])
        .on_press(Message::NavigateToSection(Section::Home))
        .style(move |_theme, _status| {
            let is_active = current_section == Section::Home;
            button::Style {
                background: Some(iced::Background::Color(
                    if is_active {
                        iced::Color::from_rgb8(59, 130, 246)
                    } else {
                        iced::Color::from_rgb8(34, 40, 71)
                    }
                )),
                text_color: iced::Color::from_rgb8(226, 232, 240),
                border: iced::Border {
                    color: if is_active {
                        iced::Color::from_rgb8(96, 165, 250)
                    } else {
                        iced::Color::from_rgb8(45, 54, 84)
                    },
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }
        });

        let create_btn = button(
            text(self.t("new_profile")).size(14)
        )
        .width(Length::Fill)
        .padding([12, 16])
        .on_press(Message::NavigateToSection(Section::CreateInstance))
        .style(move |_theme, _status| {
            let is_active = current_section == Section::CreateInstance;
            button::Style {
                background: Some(iced::Background::Color(
                    if is_active {
                        iced::Color::from_rgb8(59, 130, 246)
                    } else {
                        iced::Color::from_rgb8(34, 40, 71)
                    }
                )),
                text_color: iced::Color::from_rgb8(226, 232, 240),
                border: iced::Border {
                    color: if is_active {
                        iced::Color::from_rgb8(96, 165, 250)
                    } else {
                        iced::Color::from_rgb8(45, 54, 84)
                    },
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }
        });

        let settings_btn = button(
            text(self.t("settings")).size(14)
        )
        .width(Length::Fill)
        .padding([12, 16])
        .on_press(Message::NavigateToSection(Section::Settings))
        .style(move |_theme, _status| {
            let is_active = current_section == Section::Settings;
            button::Style {
                background: Some(iced::Background::Color(
                    if is_active {
                        iced::Color::from_rgb8(59, 130, 246)
                    } else {
                        iced::Color::from_rgb8(34, 40, 71)
                    }
                )),
                text_color: iced::Color::from_rgb8(226, 232, 240),
                border: iced::Border {
                    color: if is_active {
                        iced::Color::from_rgb8(96, 165, 250)
                    } else {
                        iced::Color::from_rgb8(45, 54, 84)
                    },
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }
        });

        let menu = column![
            home_btn,
            create_btn,
            settings_btn,
        ]
        .spacing(8)
        .padding(15);

        container(menu)
            .width(220)
            .height(Length::Fill)
            .style(|_theme| {
                container::Style {
                    background: Some(iced::Background::Color(iced::Color::from_rgb8(16, 20, 36))),
                    border: iced::Border {
                        color: iced::Color::from_rgb8(45, 54, 84),
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..Default::default()
                }
            })
            .into()
    }
    fn view_main_menu(&self) -> Element<Message> {
        let title = text(self.t("home")).size(24).color(iced::Color::from_rgb8(59, 130, 246));

        let mut profile_list = Column::new().spacing(6);

        if self.profiles.is_empty() {
            profile_list = profile_list.push(
                container(text(self.t("no_profile")).size(14).color(iced::Color::from_rgb8(148, 163, 184)))
                    .padding(20)
                    .width(Length::Fill)
                    .align_x(iced::Alignment::Center)
            );
        } else {
            for (name, profile) in &self.profiles {
                // Format playtime
                let hours = profile.playtime_seconds / 3600;
                let minutes = (profile.playtime_seconds % 3600) / 60;
                let playtime_str = if hours > 0 {
                    format!("{}h {}m", hours, minutes)
                } else if minutes > 0 {
                    format!("{}m", minutes)
                } else {
                    "No playtime".to_string()
                };

                let profile_row = container(
                    row![
                        column![
                            text(name).size(15).color(iced::Color::from_rgb8(226, 232, 240)),
                            text(format!("{} • {} MB • {}", profile.version, profile.ram_mb, playtime_str))
                                .size(12)
                                .color(iced::Color::from_rgb8(148, 163, 184)),
                        ]
                        .spacing(3)
                        .width(Length::Fill),
                        button(text(self.t("launch")).size(13))
                            .on_press(Message::LaunchProfile(name.clone()))
                            .padding([6, 14])
                            .style(|_theme, _status| {
                                button::Style {
                                    background: Some(iced::Background::Color(iced::Color::from_rgb8(16, 185, 129))),
                                    text_color: iced::Color::from_rgb8(226, 232, 240),
                                    border: iced::Border {
                                        color: iced::Color::TRANSPARENT,
                                        width: 0.0,
                                        radius: 6.0.into(),
                                    },
                                    ..Default::default()
                                }
                            }),
                        button(text(self.t("delete")).size(13))
                            .on_press(Message::DeleteProfile(name.clone()))
                            .padding([6, 14])
                            .style(|_theme, _status| {
                                button::Style {
                                    background: Some(iced::Background::Color(iced::Color::from_rgb8(239, 68, 68))),
                                    text_color: iced::Color::from_rgb8(226, 232, 240),
                                    border: iced::Border {
                                        color: iced::Color::TRANSPARENT,
                                        width: 0.0,
                                        radius: 6.0.into(),
                                    },
                                    ..Default::default()
                                }
                            }),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center)
                )
                .padding([10, 12])
                .width(Length::Fill)
                .style(|_theme| {
                    container::Style {
                        background: Some(iced::Background::Color(iced::Color::from_rgb8(34, 40, 71))),
                        border: iced::Border {
                            color: iced::Color::from_rgb8(45, 54, 84),
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    }
                });

                profile_list = profile_list.push(profile_row);
            }
        }

        let mut content = column![title]
            .spacing(15)
            .padding(0);

        if let Some(error) = &self.error_message {
            content = content.push(
                text(error)
                    .size(13)
                    .color(iced::Color::from_rgb8(239, 68, 68))
            );
        }

        content = content
            .push(
                scrollable(profile_list)
                    .height(Length::Fill)
                    .style(|_theme, _status| {
                        scrollable::Style {
                            container: container::Style::default(),
                            vertical_rail: scrollable::Rail {
                                background: None,
                                border: iced::Border::default(),
                                scroller: scrollable::Scroller {
                                    color: iced::Color::TRANSPARENT,
                                    border: iced::Border::default(),
                                },
                            },
                            horizontal_rail: scrollable::Rail {
                                background: None,
                                border: iced::Border::default(),
                                scroller: scrollable::Scroller {
                                    color: iced::Color::TRANSPARENT,
                                    border: iced::Border::default(),
                                },
                            },
                            gap: None,
                        }
                    })
            );

        content.into()
    }

    fn view_create_profile(&self) -> Element<Message> {
        let title = text(self.t("create_profile")).size(24).color(iced::Color::from_rgb8(59, 130, 246));

        let username_input = text_input(self.t("username_hint"), &self.input_username)
            .on_input(Message::UsernameChanged)
            .width(Length::Fill)
            .padding(10)
            .style(|_theme, _status| {
                text_input::Style {
                    background: iced::Background::Color(iced::Color::from_rgb8(34, 40, 71)),
                    border: iced::Border {
                        color: iced::Color::from_rgb8(45, 54, 84),
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    icon: iced::Color::from_rgb8(59, 130, 246),
                    placeholder: iced::Color::from_rgb8(100, 116, 139),
                    value: iced::Color::from_rgb8(226, 232, 240),
                    selection: iced::Color::from_rgb8(59, 130, 246),
                }
            });

        let version_picker: Element<Message> = if self.sorted_versions.is_empty() {
            text("Loading versions...").size(13).into()
        } else {
            pick_list(
                &self.sorted_versions[..],
                self.input_version.as_ref(),
                Message::VersionSelected,
            )
            .width(Length::Fill)
            .placeholder(self.t("select_version"))
            .padding(10)
            .style(|_theme, _status| {
                pick_list::Style {
                    background: iced::Background::Color(iced::Color::from_rgb8(34, 40, 71)),
                    border: iced::Border {
                        color: iced::Color::from_rgb8(45, 54, 84),
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    text_color: iced::Color::from_rgb8(226, 232, 240),
                    placeholder_color: iced::Color::from_rgb8(100, 116, 139),
                    handle_color: iced::Color::from_rgb8(59, 130, 246),
                }
            })
            .into()
        };

        let max_ram = get_total_ram_mb().unwrap_or(8192);
        let ram_slider = slider(1024..=max_ram, self.input_ram, Message::RamChanged)
            .width(Length::Fill)
            .step(512u32)
            .style(|_theme, _status| {
                slider::Style {
                    rail: slider::Rail {
                        backgrounds: (
                            iced::Background::Color(iced::Color::from_rgb8(59, 130, 246)),
                            iced::Background::Color(iced::Color::from_rgb8(45, 54, 84)),
                        ),
                        width: 4.0,
                        border: iced::Border::default(),
                    },
                    handle: slider::Handle {
                        shape: slider::HandleShape::Circle { radius: 8.0 },
                        background: iced::Background::Color(iced::Color::from_rgb8(96, 165, 250)),
                        border_width: 0.0,
                        border_color: iced::Color::TRANSPARENT,
                    },
                }
            });

        let ram_hint = text(format!("{} MB ({}: {} MB)", self.input_ram, self.t("available"), max_ram))
            .size(14)
            .color(iced::Color::from_rgb8(148, 163, 184));

        let buttons = row![
            button(text(self.t("save")).size(14))
                .on_press(Message::SaveProfile)
                .width(Length::Fill)
                .padding([8, 16])
                .style(|_theme, _status| {
                    button::Style {
                        background: Some(iced::Background::Color(iced::Color::from_rgb8(59, 130, 246))),
                        text_color: iced::Color::from_rgb8(226, 232, 240),
                        border: iced::Border {
                            color: iced::Color::TRANSPARENT,
                            width: 0.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }
                }),
            button(text(self.t("cancel")).size(14))
                .on_press(Message::CancelCreate)
                .width(Length::Fill)
                .padding([8, 16])
                .style(|_theme, _status| {
                    button::Style {
                        background: Some(iced::Background::Color(iced::Color::from_rgb8(51, 51, 51))),
                        text_color: iced::Color::from_rgb8(226, 232, 240),
                        border: iced::Border {
                            color: iced::Color::TRANSPARENT,
                            width: 0.0,
                            radius: 6.0.into(),
                        },
                        ..Default::default()
                    }
                }),
        ]
        .spacing(10);

        let username_label = text(self.t("username")).size(13).color(iced::Color::from_rgb8(148, 163, 184));
        let version_label = text(self.t("minecraft_version")).size(13).color(iced::Color::from_rgb8(148, 163, 184));
        let ram_label = text(self.t("ram_allocation")).size(13).color(iced::Color::from_rgb8(148, 163, 184));

        let mut content = column![
            title,
            Space::with_height(8),
            username_label,
            username_input,
            version_label,
            version_picker,
            ram_label,
            ram_slider,
            ram_hint,
        ]
        .spacing(12)
        .padding(0);

        if let Some(error) = &self.error_message {
            content = content.push(
                text(error)
                    .size(13)
                    .color(iced::Color::from_rgb8(239, 68, 68))
            );
        }

        content = content.push(Space::with_height(10));
        content = content.push(buttons);

        content.into()
    }

    fn t<'a>(&self, key: &'a str) -> &'a str {
    match (key, self.language) {
        // Settings
        ("settings", Language::English) => "Settings",
        ("settings", Language::Spanish) => "Configuración",
        ("language", Language::English) => "Language",
        ("language", Language::Spanish) => "Idioma",
        ("open_minecraft", Language::English) => "Open .minecraft Folder",
        ("open_minecraft", Language::Spanish) => "Abrir carpeta .minecraft",
        ("back", Language::English) => "Back",
        ("back", Language::Spanish) => "Volver",
        ("made_by", Language::English) => "Made by",
        ("made_by", Language::Spanish) => "Creado por",
        // Main menu
        ("home", Language::English) => "Home",
        ("home", Language::Spanish) => "Inicio",
        ("new_profile", Language::English) => "New Profile",
        ("new_profile", Language::Spanish) => "Nuevo Perfil",
        ("no_profile", Language::English) => "No profile, Create one to start",
        ("no_profile", Language::Spanish) => "Sin perfil, Crea uno para empezar",
        ("launch", Language::English) => "Launch",
        ("launch", Language::Spanish) => "Lanzar",
        ("delete", Language::English) => "Delete",
        ("delete", Language::Spanish) => "Eliminar",
        // Create profile
        ("create_profile", Language::English) => "Create New Profile",
        ("create_profile", Language::Spanish) => "Crear Nuevo Perfil",
        ("username", Language::English) => "Username",
        ("username", Language::Spanish) => "Nombre de usuario",
        ("username_hint", Language::English) => "Username (3-16 chars)",
        ("username_hint", Language::Spanish) => "Nombre de usuario (3-16 caracteres)",
        ("minecraft_version", Language::English) => "Minecraft Version",
        ("minecraft_version", Language::Spanish) => "Versión de Minecraft",
        ("select_version", Language::English) => "Select Minecraft version",
        ("select_version", Language::Spanish) => "Seleccionar versión de Minecraft",
        ("ram_allocation", Language::English) => "RAM Allocation",
        ("ram_allocation", Language::Spanish) => "Asignación de RAM",
        ("available", Language::English) => "Available",
        ("available", Language::Spanish) => "Disponible",
        ("save", Language::English) => "Save",
        ("save", Language::Spanish) => "Guardar",
        ("cancel", Language::English) => "Cancel",
        ("cancel", Language::Spanish) => "Cancelar",
        // Download
        ("launching", Language::English) => "Launching",
        ("launching", Language::Spanish) => "Lanzando",
        ("first_launch_msg", Language::English) => "If this is your first launch, it may take a while as the game is being downloaded",
        ("first_launch_msg", Language::Spanish) => "Si es tu primer lanzamiento, puede tardar un tiempo ya que se está descargando el juego",
        // Loading
        ("loading", Language::English) => "Loading...",
        ("loading", Language::Spanish) => "Cargando...",
        // Default
        _ => key,
    }
}
    fn view_settings(&self) -> Element<Message> {
        let title = text(self.t("settings")).size(24).color(iced::Color::from_rgb8(59, 130, 246));

        let version_text = text("RCraft v0.6")
            .size(16)
            .color(iced::Color::from_rgb8(226, 232, 240));

        let creator_text = text(format!("{} vdkvdev", self.t("made_by")))
            .size(14)
            .color(iced::Color::from_rgb8(148, 163, 184));

        let language_label = text(self.t("language"))
            .size(13)
            .color(iced::Color::from_rgb8(148, 163, 184));

        const LANGUAGES: [Language; 2] = [Language::English, Language::Spanish];
        let language_picker = pick_list(
            &LANGUAGES[..],
            Some(self.language),
            Message::LanguageSelected,
        )
        .width(Length::Fill)
        .padding(10)
        .style(|_theme, _status| {
            pick_list::Style {
                background: iced::Background::Color(iced::Color::from_rgb8(34, 40, 71)),
                border: iced::Border {
                    color: iced::Color::from_rgb8(45, 54, 84),
                    width: 1.0,
                    radius: 6.0.into(),
                },
                text_color: iced::Color::from_rgb8(226, 232, 240),
                placeholder_color: iced::Color::from_rgb8(100, 116, 139),
                handle_color: iced::Color::from_rgb8(59, 130, 246),
            }
        });

        let open_minecraft_button = button(text(self.t("open_minecraft")).size(14))
            .width(Length::Fill)
            .padding([10, 16])
            .on_press(Message::OpenMinecraftFolder)
            .style(|_theme, _status| {
                button::Style {
                    background: Some(iced::Background::Color(iced::Color::from_rgb8(59, 130, 246))),
                    text_color: iced::Color::from_rgb8(226, 232, 240),
                    border: iced::Border {
                        color: iced::Color::TRANSPARENT,
                        width: 0.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }
            });

        column![
            title,
            Space::with_height(20),
            language_label,
            Space::with_height(8),
            language_picker,
            Space::with_height(20),
            open_minecraft_button,
            Space::with_height(Length::Fill),
            version_text,
            Space::with_height(5),
            creator_text,
        ]
        .spacing(0)
        .padding(0)
            .into()
    }
}

fn main() -> iced::Result {
    iced::application(RCraftApp::title, RCraftApp::update, RCraftApp::view)
        .window(window::Settings {
            size: Size::new(900.0, 540.0),
            resizable: false,
            decorations: false,
            ..Default::default()
        })
        .run_with(RCraftApp::new)
}
