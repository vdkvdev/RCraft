use anyhow::{Context, Result};
use clap::Parser;
use console::{style, Term, Color};
use dialoguer::Input;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tokio::fs;
use std::io::Read;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Minecraft username (optional)
    #[arg(short, long)]
    username: Option<String>,

    /// Minecraft version (optional)
    #[arg(short, long, value_name = "MINECRAFT_VERSION")]
    minecraft_version: Option<String>,

    /// RAM in MB (optional)
    #[arg(short, long)]
    ram: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MinecraftVersion {
    id: String,
    #[serde(rename = "type")]
    version_type: String,
    url: String,
    time: String,
    #[serde(rename = "releaseTime")]
    release_time: String,
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
        let minecraft_dir = dirs::data_dir()
            .context("Could not find data directory")?
            .join(".minecraft");

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
        println!("{}", style("Downloading version...").cyan());

        let version_dir = self.config.versions_dir.join(&version.id);
        fs::create_dir_all(&version_dir).await?;
        let natives_dir = version_dir.join("natives");
        fs::create_dir_all(&natives_dir).await?;

        let progress = ProgressBar::new_spinner();
        progress.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {wide_msg}")
                .unwrap()
        );
        progress.set_message("Downloading version files...");

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
            downloads: VersionJsonDownloads,
            libraries: Vec<Library>,
        }
        #[derive(Deserialize)]
        struct DownloadInfo {
            url: String,
        }
        let version_json: VersionJson = serde_json::from_str(&version_data)?;
        if let Some(client) = version_json.downloads.client {
            let jar_url = client.url;
            let jar_path = version_dir.join(format!("{}.jar", version.id));
            progress.set_message("Downloading client.jar...");
            let resp = reqwest::get(&jar_url).await?;
            let bytes = resp.bytes().await?.to_vec();
            let mut out = tokio::fs::File::create(&jar_path).await?;
            use tokio::io::AsyncWriteExt;
            out.write_all(&bytes).await?;
        }
        // --- Download libraries and natives ---
        // progress.set_message("Downloading libraries and natives...");
        let os_name = "linux";
        // Clean natives folder before extracting
        if natives_dir.exists() {
            std::fs::remove_dir_all(&natives_dir)?;
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
                                        // Extract only native files (.so, .dll, .dylib)
                                        if name.ends_with(".so") || name.ends_with(".dll") || name.ends_with(".dylib") {
                                            let outpath = natives_dir.join(&name);
                                            if let Some(parent) = outpath.parent() {
                                                std::fs::create_dir_all(parent)?;
                                            }
                                            let mut outfile = std::fs::File::create(&outpath)?;
                                            std::io::copy(&mut file, &mut outfile)?;
                                            println!("Extracted native: {}", name);
                                        }
                                    }
                                    Ok::<(), anyhow::Error>(())
                                })();

                                if extraction_result.is_err() {
                                    // Suppress warnings for failed native extraction
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
        println!("{}", style("Launching Minecraft...").green());

        let java_path = self.find_java()?;
        let version_dir = self.config.versions_dir.join(version);
        let jar_path = version_dir.join(format!("{}.jar", version));
        let natives_dir = version_dir.join("natives");

        if !jar_path.exists() {
            println!("{}", style("Error: Version not downloaded").red());
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
            .arg(&self.config.assets_dir)
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
            if let Some(mut stderr) = child.stderr.take() {
                let mut err = String::new();
                stderr.read_to_string(&mut err)?;
                if err.contains("UnsatisfiedLinkError") || err.contains("lwjgl") {
                    println!("{}", style("Version not supported currently, try a version equal or superior to 1.13").red());
                } else {
                    println!("{}", style("Error running Minecraft").red());
                    println!("{}", err);
                }
            }
        }

        Ok(())
    }

    fn find_java(&self) -> Result<PathBuf> {
        // Search for Java in PATH
        if let Ok(output) = Command::new("which").arg("java").output() {
            if output.status.success() {
                let java_path = String::from_utf8(output.stdout)?;
                return Ok(PathBuf::from(java_path.trim()));
            }
        }

        // Search in common locations
        let common_paths = vec![
            "/usr/bin/java",
            "/usr/local/bin/java",
            "/opt/java/bin/java",
        ];

        for path in common_paths {
            if Path::new(path).exists() {
                return Ok(PathBuf::from(path));
            }
        }

        anyhow::bail!("Could not find installed Java")
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
    let term = Term::stdout();
    term.clear_screen()?;

    println!("{}", style(r#"
██████╗  ██████╗██████╗  █████╗ ███████╗████████╗
██╔══██╗██╔════╝██╔══██╗██╔══██╗██╔════╝╚══██╔══╝
██████╔╝██║     ██████╔╝███████║█████╗     ██║
██╔══██╗██║     ██╔══██╗██╔══██║██╔══╝     ██║
██║  ██║╚██████╗██║  ██║██║  ██║██║        ██║
 ═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝╚═╝        ╚═╝
              v0.2 - by @vdkvdev
"#).fg(Color::Yellow).bold());

    let args = Args::parse();
    let launcher = MinecraftLauncher::new()?;

    // Ensure directories exist
    launcher.config.ensure_directories().await?;

    // Get username
    let username = if let Some(name) = args.username {
        name
    } else {
        Input::<String>::new()
            .with_prompt(&style("Enter your username").fg(Color::Green).to_string())
            .validate_with(|input: &String| if input.len() >= 5 { Ok(()) } else { Err("Username must be at least 5 characters".to_string()) })
            .interact()?
    };

    // Get version
    let versions = launcher.get_available_versions().await?;
    let version = if let Some(ver) = args.minecraft_version {
        ver
    } else {
        loop {
            let input: String = Input::<String>::new()
                .with_prompt(&style("Select version. (Only from 1.13 to 1.21.8)").fg(Color::Green).to_string())
                .interact()?;
            if versions.iter().any(|v| v.id == input) {
                break input;
            } else {
                println!("Invalid version, try again.");
            }
        }
    };

    // Get RAM
    let ram_mb = if let Some(ram) = args.ram {
        ram
    } else {
        let total_ram = get_total_ram_mb()?;

        total_ram
    };

    println!("\n{}", style("Configuration:").bold());
    println!("Username: {}", username);
    println!("Version: {}", version);


    // Check if version is downloaded
    let version_dir = launcher.config.versions_dir.join(&version);
    let jar_path = version_dir.join(format!("{}.jar", version));
    let natives_dir = version_dir.join("natives");

    // Check for missing important libraries or natives
    let jopt_simple_path = launcher.config.libraries_dir.join("net/sf/jopt-simple/jopt-simple/4.6/jopt-simple-4.6.jar");
    let natives_exist = natives_dir.exists() && natives_dir.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false);
    let need_download = !jar_path.exists() || !jopt_simple_path.exists() || !natives_exist;

    if need_download {
        println!("\n{}", style("Downloading version files...").yellow());
        if let Some(target_version) = versions.iter().find(|v| v.id == version) {
            launcher.download_version(target_version).await?;
        } else {
            println!("{}", style("Error: Version not found").red());
            return Ok(());
        }
    }

    // Launch Minecraft
    launcher.launch_minecraft(&version, &username, ram_mb).await?;

    Ok(())
}
