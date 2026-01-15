use anyhow::{Result};
use std::path::PathBuf;
use tokio::fs;
use crate::models::{VersionJson};
use reqwest;
use zip;

#[derive(Clone)]
pub struct LibraryManager {
    versions_dir: PathBuf,
    libraries_dir: PathBuf,
}

impl LibraryManager {
    pub fn new(versions_dir: PathBuf) -> Self {
        let libraries_dir = versions_dir.parent().unwrap().join("libraries");
        Self { versions_dir, libraries_dir }
    }

    pub async fn check_and_download_libraries(&self, version: &str) -> Result<()> {
        let version_file = self.versions_dir.join(version).join(format!("{}.json", version));
        if !version_file.exists() {
            return Ok(());
        }
        let v_data = fs::read_to_string(&version_file).await?;
        let v_json: VersionJson = serde_json::from_str(&v_data)?;
        let os_name = crate::utils::get_os_name();

        for lib in v_json.libraries {
             if !crate::utils::is_library_allowed(&lib, os_name) {
                 continue;
             }
             
             let mut url = String::new();
             let mut path = PathBuf::new();
             
             // Try explicit artifact
             if let Some(downloads) = &lib.downloads {
                 if let Some(artifact) = &downloads.artifact {
                     url = artifact.url.clone();
                     path = self.libraries_dir.join(&artifact.path);
                 }
             }
             
             // Try Maven style if no explicit path found or url empty
             if url.is_empty() {
                 let parts: Vec<&str> = lib.name.split(':').collect();
                 if parts.len() >= 3 {
                     let group = parts[0].replace('.', "/");
                     let artifact_id = parts[1];
                     let version = parts[2];
                     let suffix = if parts.len() > 3 { format!("-{}", parts[3]) } else { "".to_string() };
                     
                     let rel_path = format!("{}/{}/{}/{}-{}{}.jar", group, artifact_id, version, artifact_id, version, suffix);
                     path = self.libraries_dir.join(&rel_path);
                     url = format!("https://libraries.minecraft.net/{}", rel_path);
                 }
             }
             
             if !url.is_empty() && !path.as_os_str().is_empty() {
                 if !path.exists() {
                     if let Some(parent) = path.parent() {
                         fs::create_dir_all(parent).await?;
                     }
                     
                     if let Ok(resp) = reqwest::get(&url).await {
                         if resp.status().is_success() {
                             if let Ok(bytes) = resp.bytes().await {
                                 fs::write(&path, &bytes).await?;
                             }
                         }
                     }
                 }
             }
        }
        Ok(())
    }

    pub async fn check_and_extract_natives(&self, natives_version: &str) -> Result<()> {
        let natives_dir = self.versions_dir.join(natives_version).join("natives");
        
        // simple check: if dir exists and is not empty, assume ok
        let natives_ok = natives_dir.exists() && std::fs::read_dir(&natives_dir).map(|c| c.count() > 0).unwrap_or(false);

        if natives_ok {
            return Ok(());
        }

        println!("Natives missing for {}, attempting repair...", natives_version);
        let version_file_native = self.versions_dir.join(natives_version).join(format!("{}.json", natives_version));
        
        if !version_file_native.exists() {
             return Ok(()); // Can't do anything if json missing
        }

        let v_data = fs::read_to_string(&version_file_native).await?;
        let v_json: VersionJson = serde_json::from_str(&v_data)?;
        let os_name = crate::utils::get_os_name();

        for lib in v_json.libraries {
            let mut native_artifact = None;
            
            // 1. Check strict 'natives' map
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
            
            // 2. heuristic: check classifiers for natives-{os}
            if native_artifact.is_none() {
                 if let Some(downloads) = &lib.downloads {
                     if let Some(classifiers) = &downloads.classifiers {
                         if let Some(artifact) = classifiers.get(&format!("natives-{}", os_name)) {
                             native_artifact = Some(artifact.clone());
                         }
                     }
                 }
            }
            
            // 3. heuristic: check main artifact path or name
            if native_artifact.is_none() {
                 if let Some(downloads) = &lib.downloads {
                     if let Some(artifact) = &downloads.artifact {
                         if artifact.path.contains(&format!("natives-{}", os_name)) || lib.name.contains(&format!("natives-{}", os_name)) {
                             native_artifact = Some(artifact.clone());
                         }
                     }
                 }
            }
            
            if let Some(artifact) = native_artifact {
                 let native_zip_path = self.versions_dir.join(natives_version).join(format!("{}.zip", lib.name.replace(":", "_")));
                 
                 // Download if missing
                 if !native_zip_path.exists() {
                    if let Ok(resp) = reqwest::get(&artifact.url).await {
                        if let Ok(bytes) = resp.bytes().await {
                             let _ = tokio::fs::write(&native_zip_path, &bytes).await;
                        }
                    }
                 }
                 
                 // Extract
                 if native_zip_path.exists() {
                     let nd = natives_dir.clone();
                     let nzp = native_zip_path.clone();
                     let exclude = lib.get_extract().map(|e| e.exclude.clone()).unwrap_or_default();
                     
                     // Spawn blocking for zip extraction
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
                                          
                                          // Create parent dirs
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
        
        Ok(())
    }
}
