//! A Vortia project on disk: a directory with a `project.vortia` manifest and a `case` folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

use crate::error::{ErrorCode, Result, VortiaError};

/// How far the simulation case of a project has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaseState {
    #[default]
    NotCreated,
    Configured,
    Meshed,
    Solved,
}

impl CaseState {
    /// The name stored in the manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotCreated => "notCreated",
            Self::Configured => "configured",
            Self::Meshed => "meshed",
            Self::Solved => "solved",
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        match text {
            "notCreated" => Ok(Self::NotCreated),
            "configured" => Ok(Self::Configured),
            "meshed" => Ok(Self::Meshed),
            "solved" => Ok(Self::Solved),
            other => Err(VortiaError::new(
                ErrorCode::InvalidData,
                format!("unknown caseState '{other}'"),
                "",
            )),
        }
    }
}

/// A project: its location, name, timestamps, the solver backend it was set up with, the
/// case setup as a JSON document and the state of the case.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    root_dir: PathBuf,
    name: String,
    created_utc: String,
    modified_utc: String,
    backend_fingerprint: String,
    setup_json: String,
    case_state: CaseState,
}

impl Project {
    pub const SCHEMA_VERSION: i64 = 1;
    pub const MANIFEST_FILE_NAME: &'static str = "project.vortia";
    pub const CASE_DIR_NAME: &'static str = "case";

    /// Creates a project in `root_dir`, which must not exist or must be empty, and writes its
    /// manifest.
    pub fn create(root_dir: &Path, name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if name.is_empty() {
            return Err(VortiaError::new(
                ErrorCode::InvalidArgument,
                "project name must not be empty",
                "",
            ));
        }
        if root_dir.exists() && !is_empty_dir(root_dir) {
            return Err(VortiaError::new(
                ErrorCode::InvalidArgument,
                "project directory already exists and is not empty",
                root_dir.display().to_string(),
            ));
        }
        fs::create_dir_all(root_dir).map_err(|e| {
            VortiaError::new(
                ErrorCode::IoError,
                "cannot create project directory",
                e.to_string(),
            )
        })?;
        let now = now_iso8601_utc();
        let mut project = Self {
            root_dir: absolute(root_dir)?,
            name,
            created_utc: now.clone(),
            modified_utc: now,
            backend_fingerprint: String::new(),
            setup_json: "{}".to_owned(),
            case_state: CaseState::NotCreated,
        };
        project.save()?;
        Ok(project)
    }

    /// Opens the project in `root_dir`. A manifest with missing or wrongly typed fields is
    /// reported as an error, never accepted with defaults for required fields.
    pub fn open(root_dir: &Path) -> Result<Self> {
        let manifest = root_dir.join(Self::MANIFEST_FILE_NAME);
        let shown = manifest.display().to_string();
        if !manifest.is_file() {
            return Err(VortiaError::new(
                ErrorCode::NotFound,
                "not a Vortia project (manifest missing)",
                shown,
            ));
        }
        let text = fs::read_to_string(&manifest).map_err(|_| {
            VortiaError::new(
                ErrorCode::IoError,
                "cannot open manifest for reading",
                shown.clone(),
            )
        })?;
        let document: Value = serde_json::from_str(&text)
            .map_err(|e| VortiaError::new(ErrorCode::ParseError, e.to_string(), shown.clone()))?;
        let invalid =
            |message: String| VortiaError::new(ErrorCode::InvalidData, message, shown.clone());
        let fields = document
            .as_object()
            .ok_or_else(|| invalid("project manifest is not a JSON object".to_owned()))?;

        let schema = match fields.get("schemaVersion") {
            None => -1,
            Some(value) => value
                .as_i64()
                .ok_or_else(|| invalid("schemaVersion is not an integer".to_owned()))?,
        };
        if schema != Self::SCHEMA_VERSION {
            return Err(VortiaError::new(
                ErrorCode::UnsupportedVersion,
                format!("unsupported project schemaVersion {schema}"),
                shown,
            ));
        }
        let text_field = |key: &str, default: &str| -> Result<String> {
            match fields.get(key) {
                None => Ok(default.to_owned()),
                Some(Value::String(text)) => Ok(text.clone()),
                Some(_) => Err(invalid(format!("{key} is not a string"))),
            }
        };
        let name = text_field("name", "")?;
        if name.is_empty() {
            return Err(invalid("project manifest has no name".to_owned()));
        }
        Ok(Self {
            root_dir: absolute(root_dir)?,
            name,
            created_utc: text_field("created", "")?,
            modified_utc: text_field("modified", "")?,
            backend_fingerprint: text_field("backendFingerprint", "")?,
            setup_json: fields
                .get("setup")
                .map_or_else(|| "{}".to_owned(), Value::to_string),
            case_state: CaseState::parse(&text_field("caseState", "notCreated")?)?,
        })
    }

    /// Writes the manifest, replacing the previous one only once the new one is complete.
    pub fn save(&mut self) -> Result<()> {
        if self.root_dir.as_os_str().is_empty() {
            return Err(VortiaError::new(
                ErrorCode::InvalidArgument,
                "project has no root directory",
                "",
            ));
        }
        let setup: Value = serde_json::from_str(&self.setup_json).map_err(|e| {
            VortiaError::new(
                ErrorCode::InvalidData,
                format!("setup payload is not valid JSON: {e}"),
                "",
            )
        })?;
        self.modified_utc = now_iso8601_utc();
        let mut fields = Map::new();
        fields.insert(
            "schemaVersion".to_owned(),
            Value::from(Self::SCHEMA_VERSION),
        );
        fields.insert("name".to_owned(), Value::from(self.name.clone()));
        fields.insert("created".to_owned(), Value::from(self.created_utc.clone()));
        fields.insert(
            "modified".to_owned(),
            Value::from(self.modified_utc.clone()),
        );
        fields.insert(
            "backendFingerprint".to_owned(),
            Value::from(self.backend_fingerprint.clone()),
        );
        fields.insert(
            "caseState".to_owned(),
            Value::from(self.case_state.as_str()),
        );
        fields.insert("setup".to_owned(), setup);
        let mut contents = serde_json::to_string_pretty(&Value::Object(fields))
            .map_err(|e| VortiaError::new(ErrorCode::Internal, e.to_string(), ""))?;
        contents.push('\n');
        write_file_atomic(&self.manifest_path(), &contents)
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root_dir.join(Self::MANIFEST_FILE_NAME)
    }

    pub fn case_dir(&self) -> PathBuf {
        self.root_dir.join(Self::CASE_DIR_NAME)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Renames the project. An empty name is ignored.
    pub fn set_name(&mut self, name: impl Into<String>) {
        let name = name.into();
        if !name.is_empty() {
            self.name = name;
        }
    }

    pub fn setup_json(&self) -> &str {
        &self.setup_json
    }

    pub fn set_setup_json(&mut self, json: impl Into<String>) {
        self.setup_json = json.into();
    }

    pub fn backend_fingerprint(&self) -> &str {
        &self.backend_fingerprint
    }

    pub fn set_backend_fingerprint(&mut self, fingerprint: impl Into<String>) {
        self.backend_fingerprint = fingerprint.into();
    }

    pub fn case_state(&self) -> CaseState {
        self.case_state
    }

    pub fn set_case_state(&mut self, state: CaseState) {
        self.case_state = state;
    }
}

fn is_empty_dir(path: &Path) -> bool {
    fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
}

fn absolute(path: &Path) -> Result<PathBuf> {
    std::path::absolute(path).map_err(|e| {
        VortiaError::new(
            ErrorCode::IoError,
            "cannot resolve project path",
            e.to_string(),
        )
    })
}

fn write_file_atomic(path: &Path, contents: &str) -> Result<()> {
    let mut temp_name = path.file_name().unwrap_or_default().to_os_string();
    temp_name.push(".tmp");
    let temp = path.with_file_name(temp_name);
    fs::write(&temp, contents).map_err(|_| {
        VortiaError::new(
            ErrorCode::IoError,
            "failed writing temp manifest",
            temp.display().to_string(),
        )
    })?;
    fs::rename(&temp, path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        VortiaError::new(
            ErrorCode::IoError,
            "failed to rename manifest into place",
            e.to_string(),
        )
    })
}

/// The current time as `YYYY-MM-DDTHH:MM:SSZ`.
fn now_iso8601_utc() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    iso8601_utc(seconds)
}

/// Seconds since the Unix epoch as a UTC timestamp (proleptic Gregorian calendar).
fn iso8601_utc(seconds: u64) -> String {
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // Civil date from a day count, after Howard Hinnant's `civil_from_days`.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    /// A private directory that does not exist yet, removed when the guard is dropped.
    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos());
            let name = format!(
                "vortia-test-{}-{}-{nanos}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            Self(std::env::temp_dir().join(name))
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn create_persists_manifest() {
        let root = TempRoot::new();
        let project = Project::create(&root.0, "cavity").expect("created");
        assert!(project.manifest_path().is_file());
        assert_eq!(project.name(), "cavity");
        assert_eq!(project.case_state(), CaseState::NotCreated);
    }

    #[test]
    fn open_round_trip() {
        let root = TempRoot::new();
        {
            let mut project = Project::create(&root.0, "cavity").expect("created");
            project.set_backend_fingerprint("esi/v2606@/opt/OpenFOAM-v2606");
            project.set_setup_json(r#"{"geometry":{"kind":"box"}}"#);
            project.set_case_state(CaseState::Meshed);
            project.save().expect("saved");
        }
        let reopened = Project::open(&root.0).expect("opened");
        assert_eq!(reopened.name(), "cavity");
        assert_eq!(
            reopened.backend_fingerprint(),
            "esi/v2606@/opt/OpenFOAM-v2606"
        );
        assert_eq!(reopened.case_state(), CaseState::Meshed);
        assert!(reopened.setup_json().contains("box"));
    }

    #[test]
    fn refuses_non_empty_directory() {
        let root = TempRoot::new();
        fs::create_dir_all(&root.0).expect("directory");
        fs::write(root.0.join("precious.txt"), "do not touch").expect("file");
        assert_eq!(
            Project::create(&root.0, "cavity")
                .expect_err("refused")
                .code,
            ErrorCode::InvalidArgument
        );
        assert!(root.0.join("precious.txt").is_file());
    }

    #[test]
    fn detects_corrupted_manifest() {
        let root = TempRoot::new();
        let project = Project::create(&root.0, "cavity").expect("created");
        fs::write(project.manifest_path(), "{ this is not json").expect("written");
        assert_eq!(
            Project::open(&root.0).expect_err("corrupted").code,
            ErrorCode::ParseError
        );
    }

    #[test]
    fn rejects_unsupported_schema() {
        let root = TempRoot::new();
        let project = Project::create(&root.0, "cavity").expect("created");
        fs::write(
            project.manifest_path(),
            r#"{"schemaVersion": 9999, "name": "future"}"#,
        )
        .expect("written");
        assert_eq!(
            Project::open(&root.0).expect_err("unsupported").code,
            ErrorCode::UnsupportedVersion
        );
    }

    #[test]
    fn rejects_wrong_manifest_types() {
        let root = TempRoot::new();
        let project = Project::create(&root.0, "cavity").expect("created");
        for content in [
            "[]",
            "null",
            r#"{"schemaVersion":"1","name":"test"}"#,
            r#"{"schemaVersion":1,"name":[]}"#,
            r#"{"schemaVersion":1,"name":"test","backendFingerprint":42}"#,
            r#"{"schemaVersion":1,"name":"test","caseState":false}"#,
        ] {
            fs::write(project.manifest_path(), content).expect("written");
            assert_eq!(
                Project::open(&root.0).expect_err("invalid").code,
                ErrorCode::InvalidData,
                "{content}"
            );
        }
    }

    #[test]
    fn open_fails_on_missing_manifest() {
        let root = TempRoot::new();
        fs::create_dir_all(&root.0).expect("directory");
        assert_eq!(
            Project::open(&root.0).expect_err("missing").code,
            ErrorCode::NotFound
        );
    }

    #[test]
    fn rejects_empty_name() {
        let root = TempRoot::new();
        assert_eq!(
            Project::create(&root.0, "").expect_err("empty").code,
            ErrorCode::InvalidArgument
        );
    }

    #[test]
    fn case_state_names_round_trip() {
        for state in [
            CaseState::NotCreated,
            CaseState::Configured,
            CaseState::Meshed,
            CaseState::Solved,
        ] {
            assert_eq!(CaseState::parse(state.as_str()).expect("known"), state);
        }
        assert_eq!(
            CaseState::parse("frozen").expect_err("unknown").code,
            ErrorCode::InvalidData
        );
    }

    #[test]
    fn timestamps_are_iso_8601_utc() {
        assert_eq!(iso8601_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso8601_utc(1_794_268_799), "2026-11-09T23:59:59Z");
    }

    #[test]
    fn manifest_is_compatible_with_the_cpp_application() {
        let root = TempRoot::new();
        let project = Project::create(&root.0, "cavity").expect("created");
        let text = fs::read_to_string(project.manifest_path()).expect("manifest");
        let manifest: Value = serde_json::from_str(&text).expect("json");
        for key in [
            "backendFingerprint",
            "caseState",
            "created",
            "modified",
            "name",
            "schemaVersion",
            "setup",
        ] {
            assert!(manifest.get(key).is_some(), "manifest has {key}");
        }
        assert!(text.ends_with("}\n"));
    }
}
