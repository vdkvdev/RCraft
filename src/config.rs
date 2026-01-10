use anyhow::Result;
use std::env;
use std::path::PathBuf;


#[derive(Clone)]
pub struct LauncherConfig {
    pub minecraft_dir: PathBuf,
    pub versions_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub libraries_dir: PathBuf,
    pub runtimes_dir: PathBuf,
}

impl LauncherConfig {
    pub fn new() -> Result<Self> {
        let home = env::var("HOME").unwrap();
        let minecraft_dir = PathBuf::from(home).join(".minecraft");

        Ok(Self {
            versions_dir: minecraft_dir.join("versions"),
            assets_dir: minecraft_dir.join("assets"),
            libraries_dir: minecraft_dir.join("libraries"),
            runtimes_dir: minecraft_dir.join("runtimes"),
            minecraft_dir,
        })
    }


}
