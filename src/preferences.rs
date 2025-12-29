use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Preferences {
    pub summary_language: String,

    #[serde(skip)]
    file_path: Option<String>,
}

impl Preferences {
    pub fn ephemeral() -> Self {
        Self {
            summary_language: "English".to_string(),
            file_path: None,
        }
    }

    pub fn with_file(file_path: &str) -> Self {
        Self {
            summary_language: "English".to_string(),
            file_path: Some(file_path.to_string()),
        }
    }

    pub fn load_or_ephemeral(file_path: Option<&str>) -> Self {
        match file_path {
            Some(path) => Self::load_from_file(path).unwrap_or_else(|e| {
                log::error!("Failed to load preferences from {path}: {e}");
                Self::with_file(path)
            }),
            None => Self::ephemeral(),
        }
    }

    pub fn load_from_file(file_path: &str) -> anyhow::Result<Self> {
        let path = Path::new(file_path);
        if path.exists() {
            let content = fs::read_to_string(path)?;

            match serde_json::from_str::<Self>(&content) {
                Ok(mut prefs) => {
                    prefs.file_path = Some(file_path.to_string());
                    Ok(prefs)
                }
                Err(e) => {
                    log::error!("Failed to parse preferences file: {e}");
                    Err(anyhow::anyhow!("Failed to parse preferences: {}", e))
                }
            }
        } else {
            Ok(Self::with_file(file_path))
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        match &self.file_path {
            Some(path) => {
                let content = serde_json::to_string_pretty(self)?;
                fs::write(path, content)?;
                Ok(())
            }
            None => Ok(()),
        }
    }
}
