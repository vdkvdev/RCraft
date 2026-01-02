use reqwest::Client;
use std::path::PathBuf;
use std::fs::File;
use std::io::Write;
use crate::models::{ModSearchResult, ModVersion};

const MODRINTH_API_URL: &str = "https://api.modrinth.com/v2";

#[derive(Clone)]
pub struct ModrinthClient {
    client: Client,
}

impl ModrinthClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("rcraft/0.9.0 (rcraft@gmail.com)") // fake email. just for modrinth
                .build()
                .unwrap_or_default(),
        }
    }

    pub async fn search_mods(&self, query: &str, limit: u32, version: Option<&str>, loader: Option<&str>) -> Result<Vec<ModSearchResult>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/search", MODRINTH_API_URL);

        #[derive(serde::Deserialize)]
        struct SearchResponse {
            hits: Vec<ModSearchResult>,
        }

        let retries = 3;
        let mut delay = 1;
        let mut last_error = None;

        let mut facets = Vec::new();
        if let Some(v) = version {
            facets.push(format!("versions:{}", v));
        }
        if let Some(l) = loader {
            facets.push(format!("categories:{}", l));
        }

        let facets_json = if !facets.is_empty() {
             let f: Vec<Vec<String>> = facets.into_iter().map(|s| vec![s]).collect();
             serde_json::to_string(&f).unwrap_or_default()
        } else {
             String::new()
        };

        for _ in 0..=retries {
             let mut request = self.client.get(&url)
                .query(&[("query", query), ("limit", &limit.to_string())]);

             if !facets_json.is_empty() {
                 request = request.query(&[("facets", &facets_json)]);
             }

            match request.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                         let resp = response.json::<SearchResponse>().await?;
                         return Ok(resp.hits);
                    } else if response.status().is_server_error() {
                        // 5xx error, retry
                        let status = response.status();
                        if status.as_u16() == 503 {
                            return Err("Modrinth Service Unavailable (503). Please try again later.".into());
                        }
                        let text = response.text().await.unwrap_or_default();
                        last_error = Some(format!("Modrinth API error: {} - {}", status, text));
                    } else {
                        // 4xx error, don't retry
                        let status = response.status();
                        let text = response.text().await?;
                         if text.len() > 200 || text.contains("<html") {
                            return Err(format!("Modrinth API error: {}", status).into());
                        }
                        return Err(format!("Modrinth API error: {} - {}", status, text).into());
                    }
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            delay *= 2;
        }

        Err(last_error.unwrap_or_else(|| "Unknown error".to_string()).into())
    }

    pub async fn get_versions(&self, project_id: &str, loader: Option<&str>, game_version: Option<&str>) -> Result<Vec<ModVersion>, Box<dyn std::error::Error + Send + Sync>> {
        let retries = 3;
        let mut delay = 1;
        let mut last_error = None;

        for _ in 0..=retries {
             let url = format!("{}/project/{}/version", MODRINTH_API_URL, project_id);
             let mut request = self.client.get(&url);

             let mut params = Vec::new();
             if let Some(l) = loader {
                 params.push(("loaders", format!("[\"{}\"]", l)));
             }
             if let Some(v) = game_version {
                 params.push(("game_versions", format!("[\"{}\"]", v)));
             }

             request = request.query(&params);

            match request.send().await {
                Ok(response) => {
                     if response.status().is_success() {
                         let resp = response.json::<Vec<ModVersion>>().await?;
                         return Ok(resp);
                     } else if response.status().is_server_error() {
                         let status = response.status();
                         if status.as_u16() == 503 {
                            return Err("Modrinth Service Unavailable (503). Please try again later.".into());
                         }
                         let text = response.text().await.unwrap_or_default();
                         last_error = Some(format!("Modrinth API error: {} - {}", status, text));
                     } else {
                         let status = response.status();
                         let text = response.text().await?;
                         if text.len() > 200 || text.contains("<html") {
                            return Err(format!("Modrinth API error: {}", status).into());
                        }
                        return Err(format!("Modrinth API error: {} - {}", status, text).into());
                     }
                }
                 Err(e) => {
                    last_error = Some(e.to_string());
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            delay *= 2;
        }

        Err(last_error.unwrap_or_else(|| "Unknown error".to_string()).into())
    }

    pub async fn download_mod(&self, url: &str, destination: &PathBuf) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let response = self.client.get(url).send().await?;
        let bytes = response.bytes().await?;

        let mut file = File::create(destination)?;
        file.write_all(&bytes)?;

        Ok(())
    }

    pub async fn download_icon(&self, url: &str, destination: &PathBuf) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let response = self.client.get(url).send().await?;
        let bytes = response.bytes().await?;

        let mut file = File::create(destination)?;
        file.write_all(&bytes)?;

        Ok(())
    }

    pub async fn download_icon_bytes(&self, url: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let response = self.client.get(url).send().await?;
        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }
}
