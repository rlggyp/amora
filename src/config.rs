use crate::Error;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize, Debug, Default)]
pub struct Cors {
    #[serde(default)]
    pub allow_credentials: bool,
    #[serde(default)]
    pub allow_origins: Vec<String>,
    #[serde(default)]
    pub allow_methods: Vec<String>,
    #[serde(default)]
    pub allow_headers: Vec<String>,
}

#[derive(Deserialize, Debug, Default)]
pub struct Config {
    #[serde(default)]
    pub cors: Cors,
    pub bearer_token: Option<String>,
    pub keep_alive_interval: Option<u64>,
}

impl Config {
    pub fn get_config() -> Result<Config, Error> {
        let config_path = std::env::var("CONFIG_FILE")
            .unwrap_or_else(|_| "configs/config.yaml".to_string());

        let mut config = if Path::new(&config_path).exists() {
            let file = std::fs::File::open(&config_path)?;
            serde_yaml::from_reader(file)?
        } else {
            Config::default()
        };

        if let Ok(env_token) = std::env::var("AMORA_BEARER_TOKEN") {
            if !env_token.is_empty() {
                config.bearer_token = Some(env_token);
            }
        }

        if let Ok(env_interval) = std::env::var("AMORA_KEEP_ALIVE_INTERVAL")
            .or_else(|_| std::env::var("KEEP_ALIVE_INTERVAL"))
        {
            if let Ok(secs) = env_interval.parse::<u64>() {
                config.keep_alive_interval = Some(secs);
            }
        }

        Ok(config)
    }

    pub fn get_port() -> u16 {
        std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(12000)
    }

    pub fn get_base_path() -> Option<String> {
        std::env::var("AMORA_BASE_PATH")
            .ok()
            .and_then(|s| Self::normalize_base_path(&s))
    }

    pub fn normalize_base_path(raw: &str) -> Option<String> {
        let trimmed = raw.trim_matches('/');
        if trimmed.is_empty() {
            None
        } else {
            Some(format!("/{}", trimmed))
        }
    }

    pub fn get_keep_alive_interval(&self) -> u64 {
        self.keep_alive_interval.unwrap_or(15)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_base_path() {
        assert_eq!(Config::normalize_base_path("amora"), Some("/amora".to_string()));
        assert_eq!(Config::normalize_base_path("/amora"), Some("/amora".to_string()));
        assert_eq!(Config::normalize_base_path("/amora/"), Some("/amora".to_string()));
        assert_eq!(Config::normalize_base_path("///"), None);
        assert_eq!(Config::normalize_base_path(""), None);
        assert_eq!(Config::normalize_base_path("team/amora"), Some("/team/amora".to_string()));
    }

    #[test]
    fn test_keep_alive_interval_default() {
        let cfg = Config::default();
        assert_eq!(cfg.get_keep_alive_interval(), 15);
    }
}
