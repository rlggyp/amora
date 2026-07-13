use crate::Error;
use serde::Deserialize;

use std::collections::HashMap;

#[derive(Deserialize, Debug)]
pub struct Cors {
    pub allow_credentials: bool,
    pub allow_origins: Vec<String>,
    pub allow_methods: Vec<String>,
    pub allow_headers: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub cors: Cors,
    pub basic_auth_users: HashMap<String, String>,
}

impl Config {
    pub fn get_config() -> Result<Config, Error> {
        let config_file = std::env::var("CONFIG_FILE")
            .map_err(|e| {
                let error = format!("Environment variable `CONFIG_FILE` not found {}", e);
                log::error!("{}", error);
                error
            })?;

        let file = std::fs::File::open(config_file)
            .map_err(|e| {
                log::error!("failed to open config file: {}", e);
                e
            })?;

        let config: Config = serde_yaml::from_reader(file)?;

        Ok(config)
    }
}