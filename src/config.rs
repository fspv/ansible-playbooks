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

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub users: BTreeMap<String, UserSpec>,
    pub nvidia: bool,
    pub system_vendor: Option<String>,
    pub ca_cert: BTreeMap<String, String>,
    pub iptables_open_ports: IptablesPorts,
    /// Ubuntu archive components to enable. Mirrors
    /// `roles/apt/defaults/main.yml: apt_repos`. Each entry maps to a pin
    /// file under `/etc/apt/preferences.d/<name>.pref` and a sources file
    /// under `/etc/apt/sources.list.d/<name>.list`. Unknown values fail at
    /// `Config::load` (serde rejects them with an enum-of-strings error).
    pub apt_repos: Vec<AptRepo>,
}

/// Closed set of recognised `apt_repos` entries. Names match the legacy
/// ansible templates under `roles/apt/templates/etc/apt/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum AptRepo {
    #[serde(rename = "ubuntu")]
    Ubuntu,
    #[serde(rename = "ubuntu-security")]
    UbuntuSecurity,
    #[serde(rename = "ubuntu-updates")]
    UbuntuUpdates,
    #[serde(rename = "ubuntu-backports")]
    UbuntuBackports,
    #[serde(rename = "ubuntu-proposed")]
    UbuntuProposed,
    #[serde(rename = "ppa-pv-safronov-backports")]
    PpaPvSafronovBackports,
}

impl AptRepo {
    /// File-name stem used for both `<stem>.pref` and `<stem>.list`.
    #[must_use]
    pub const fn stem(&self) -> &'static str {
        match self {
            Self::Ubuntu => "ubuntu",
            Self::UbuntuSecurity => "ubuntu-security",
            Self::UbuntuUpdates => "ubuntu-updates",
            Self::UbuntuBackports => "ubuntu-backports",
            Self::UbuntuProposed => "ubuntu-proposed",
            Self::PpaPvSafronovBackports => "ppa-pv-safronov-backports",
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            users: BTreeMap::new(),
            nvidia: false,
            system_vendor: None,
            ca_cert: BTreeMap::new(),
            iptables_open_ports: IptablesPorts::default(),
            apt_repos: default_apt_repos(),
        }
    }
}

fn default_apt_repos() -> Vec<AptRepo> {
    vec![
        AptRepo::Ubuntu,
        AptRepo::UbuntuSecurity,
        AptRepo::UbuntuUpdates,
        AptRepo::UbuntuBackports,
    ]
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct IptablesPorts {
    pub remote: IptablesPortsBySection,
    pub local: IptablesPortsBySection,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct IptablesPortsBySection {
    pub tcp: Vec<u16>,
    pub udp: Vec<u16>,
}

#[derive(Debug, Clone, Deserialize)]
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
