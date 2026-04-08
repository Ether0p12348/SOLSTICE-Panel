use crate::config::AppConfig;
use anyhow::{Context, Result};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

pub type SharedConfigStore = Arc<RwLock<ConfigStore>>;

#[derive(Debug, Clone)]
pub struct ConfigStore {
    pub path: PathBuf,
    pub config: AppConfig,
}

impl ConfigStore {
    pub fn save(&self) -> Result<()> {
        self.config
            .save_to_file(&self.path)
            .with_context(|| format!("failed saving config to {}", self.path.display()))
    }

    pub fn replace_config(&mut self, config: AppConfig) -> Result<()> {
        config.validate()?;
        self.config = config;
        self.save()
    }

    pub fn config_toml_pretty(&self) -> Result<String> {
        self.config.to_toml_pretty()
    }
}
