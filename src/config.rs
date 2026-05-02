use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::Error;

// `users:` is a map keyed by user name to match the existing legacy ansible
// config (manual/common.yml). The map key is the canonical user name; any
// inner `user:` field (which the legacy config redundantly carries) is
// silently ignored, as are other ansible-only fields like `custom_secrets`,
// `custom_secrets_from_file`, and `gpg_keys` — serde drops unknown fields by
// default, so existing files parse without modification.

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub users: BTreeMap<String, UserSpec>,
}

#[derive(Debug, Deserialize)]
pub struct UserSpec {
    #[serde(default)]
    pub uid: Option<u32>,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub home: Option<PathBuf>,
    #[serde(default)]
    pub shell: Option<PathBuf>,
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub password: Option<String>,
}

impl Config {
    /// Load and parse a YAML config file.
    ///
    /// # Errors
    /// Returns [`Error::ConfigLoad`] if the file cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let raw = std::fs::read_to_string(path).map_err(|e| Error::ConfigLoad {
            path: path.to_path_buf(),
            source: Box::new(e),
        })?;
        serde_yaml::from_str(&raw).map_err(|e| Error::ConfigLoad {
            path: path.to_path_buf(),
            source: Box::new(e),
        })
    }
}
