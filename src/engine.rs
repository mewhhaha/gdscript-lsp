use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Version {
    #[value(name = "4.6")]
    V4_6,
    #[value(name = "4.7")]
    V4_7,
}

impl Default for Version {
    fn default() -> Self {
        Self::V4_6
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Version {
    type Err = &'static str;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::from_raw(raw).ok_or("unknown Godot version")
    }
}

impl Version {
    pub fn from_raw(raw: &str) -> Option<Self> {
        match raw
            .trim()
            .trim_start_matches('v')
            .replace('_', ".")
            .as_str()
        {
            "4.6" => Some(Self::V4_6),
            "4.7" => Some(Self::V4_7),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V4_6 => "4.6",
            Self::V4_7 => "4.7",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum BehaviorMode {
    /// Run only parity-level rules/actions.
    #[value(name = "parity")]
    Parity,
    /// Run parity rules plus enhanced diagnostics/actions.
    #[value(name = "enhanced")]
    Enhanced,
}

impl Default for BehaviorMode {
    fn default() -> Self {
        Self::Enhanced
    }
}

impl BehaviorMode {
    pub fn from_raw(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "parity" => Some(Self::Parity),
            "enhanced" => Some(Self::Enhanced),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngineConfig {
    pub godot_version: Version,
    pub behavior_mode: BehaviorMode,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            godot_version: Version::default(),
            behavior_mode: BehaviorMode::default(),
        }
    }
}
