use crate::error::{ErrorCode, Result, VortiaError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum CaseState {
    NotCreated,
    Configured,
    Meshed,
    Solved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectManifest {
    pub schema_version: i32,
    pub name: String,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    #[serde(default)]
    pub backend_fingerprint: String,
    #[serde(default = "default_case_state")]
    pub case_state: CaseState,
    #[serde(default = "default_setup")]
    pub setup: serde_json::Value,
}

fn default_case_state() -> CaseState {
    CaseState::NotCreated
}
fn default_setup() -> serde_json::Value {
    serde_json::json!({})
}

pub const SCHEMA_VERSION: i32 = 1;
pub const MANIFEST_FILE: &str = "project.vortia";

#[derive(Debug)]
pub struct Project {
    pub root: PathBuf,
    pub manifest: ProjectManifest,
}

impl Project {
    pub fn create(root: impl AsRef<Path>, name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if name.is_empty() {
            return Err(VortiaError::new(
                ErrorCode::InvalidArgument,
                "project name must not be empty",
                "",
            ));
        }
        let root = root.as_ref().to_path_buf();
        if root.exists() && fs::read_dir(&root).map(|mut d| d.next().is_some()).unwrap_or(false) {
            return Err(VortiaError::new(
                ErrorCode::InvalidArgument,
                "project directory already exists and is not empty",
                root.display().to_string(),
            ));
        }
        fs::create_dir_all(&root).map_err(|e| {
            VortiaError::new(ErrorCode::IoError, e.to_string(), root.display().to_string())
        })?;
        let now = Utc::now();
        let manifest = ProjectManifest {
            schema_version: SCHEMA_VERSION,
            name: name.clone(),
            created: now,
            modified: now,
            backend_fingerprint: String::new(),
            case_state: CaseState::NotCreated,
            setup: serde_json::json!({}),
        };
        let mut project = Self { root, manifest };
        project.save()?;
        Ok(project)
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let manifest_path = root.join(MANIFEST_FILE);
        if !manifest_path.is_file() {
            return Err(VortiaError::new(
                ErrorCode::NotFound,
                "not a Vortia project (manifest missing)",
                manifest_path.display().to_string(),
            ));
        }
        let text = fs::read_to_string(&manifest_path).map_err(|e| {
            VortiaError::new(ErrorCode::IoError, e.to_string(), manifest_path.display().to_string())
        })?;
        let manifest: ProjectManifest = serde_json::from_str(&text).map_err(|e| {
            VortiaError::new(ErrorCode::ParseError, e.to_string(), manifest_path.display().to_string())
        })?;
        if manifest.schema_version != SCHEMA_VERSION {
            return Err(VortiaError::new(
                ErrorCode::UnsupportedVersion,
                format!("unsupported schemaVersion {}", manifest.schema_version),
                manifest_path.display().to_string(),
            ));
        }
        if manifest.name.is_empty() {
            return Err(VortiaError::new(
                ErrorCode::InvalidData,
                "project manifest has no name",
                manifest_path.display().to_string(),
            ));
        }
        Ok(Self { root, manifest })
    }

    pub fn save(&mut self) -> Result<()> {
        self.manifest.modified = Utc::now();
        let text = serde_json::to_string_pretty(&self.manifest).map_err(|e| {
            VortiaError::new(ErrorCode::Internal, e.to_string(), "")
        })?;
        let manifest_path = self.root.join(MANIFEST_FILE);
        let tmp = self.root.join(format!("{}.tmp", MANIFEST_FILE));
        fs::write(&tmp, text.as_bytes()).map_err(|e| {
            VortiaError::new(ErrorCode::IoError, e.to_string(), tmp.display().to_string())
        })?;
        fs::rename(&tmp, &manifest_path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            VortiaError::new(ErrorCode::IoError, e.to_string(), manifest_path.display().to_string())
        })?;
        Ok(())
    }

    pub fn case_dir(&self) -> PathBuf {
        self.root.join("case")
    }
}