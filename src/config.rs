//! ~/.config/sniffr/config.toml (XDG, matching the bash — not macOS App Support).
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub backend: Option<String>,
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub max: Option<u32>,
    pub min_severity: Option<String>,
    pub min_confidence: Option<f64>,
    #[serde(default)]
    pub consensus: ConsensusConfig,
    #[serde(default)]
    pub backends: HashMap<String, CustomBackend>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ConsensusConfig {
    pub model: Option<String>,
    pub agent: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomBackend {
    pub open: Option<String>,
    pub inject: Option<String>,
}

impl Config {
    pub fn dir() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"))
            .join("sniffr")
    }
    pub fn path() -> PathBuf {
        Self::dir().join("config.toml")
    }
    /// Load the config, or defaults if missing/unparseable.
    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }
}
