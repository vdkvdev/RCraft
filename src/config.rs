// config.rs - Configuration and file system operations

use anyhow::Result;
use std::env;
use std::path::PathBuf;
use tokio::fs;

#[derive(Clone)]
pub struct LauncherConfig {
    pub minecraft_dir: PathBuf,
    pub versions_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub libraries_dir: PathBuf,
}

impl LauncherConfig {
    pub fn new() -> Result<Self> {
        let home = env::var("HOME").unwrap();
        let minecraft_dir = PathBuf::from(home).join(".minecraft");

        Ok(Self {
            versions_dir: minecraft_dir.join("versions"),
            assets_dir: minecraft_dir.join("assets"),
            libraries_dir: minecraft_dir.join("libraries"),
            minecraft_dir,
        })
    }

    pub async fn ensure_directories(&self) -> Result<()> {
        fs::create_dir_all(&self.minecraft_dir).await?;
        fs::create_dir_all(&self.versions_dir).await?;
        fs::create_dir_all(&self.assets_dir).await?;
        fs::create_dir_all(&self.libraries_dir).await?;
        Ok(())
    }
}