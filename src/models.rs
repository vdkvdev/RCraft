use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
    pub url: String,
    pub time: String,
    #[serde(rename = "releaseTime")]
    pub release_time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionManifest {
    pub versions: Vec<MinecraftVersion>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Extract {
    pub exclude: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct OsRule {
    pub name: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LibraryArtifact {
    pub url: String,
    pub path: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LibraryDownloads {
    pub artifact: Option<LibraryArtifact>,
    pub classifiers: Option<HashMap<String, LibraryArtifact>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub natives: Option<HashMap<String, String>>,
    pub rules: Option<Vec<Rule>>,
    #[serde(default)]
    pub extract: Option<Extract>,
}

impl Library {
    pub fn get_extract(&self) -> Option<&Extract> {
        self.extract.as_ref()
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct AssetIndex {
    pub id: String,
    pub url: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AssetIndexJson {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AssetObject {
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub username: String,
    pub version: String,
    pub ram_mb: u32,
    #[serde(default)]
    pub playtime_seconds: u64,
    #[serde(default)]
    pub last_launch: Option<u64>,
    #[serde(default)]
    pub is_fabric: bool,
    #[serde(default)]
    pub game_dir: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Section {
    Home,
    CreateInstance,
    Settings,
    Logs,
    Mods,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
    System,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::System
    }
}

impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Theme::Dark => write!(f, "Dark"),
            Theme::Light => write!(f, "Light"),
            Theme::System => write!(f, "System"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModSearchResult {
    pub project_id: String,
    pub title: String,
    pub description: Option<String>,
    pub author: String,
    pub icon_url: Option<String>,
    pub versions: Option<Vec<String>>,
    pub follows: u32,
    pub downloads: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModVersion {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<ModFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModFile {
    pub hashes: ModFileHashes,
    pub url: String,
    pub filename: String,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModFileHashes {
    pub sha1: String,
    pub sha512: String,
}
