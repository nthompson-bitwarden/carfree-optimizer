use super::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Number, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub(super) const MAX_BYTES: u64 = 32 * 1024 * 1024;

// The shared serde_json float parser is part of earlier ingestion snapshot
// identities. Use Rust's correctly rounded parser for workspace values only.
pub fn exact_json(input: &str) -> serde_json::Result<Value> {
    fn parse(input: &str, depth: u8) -> serde_json::Result<Value> {
        use serde::de::Error as _;
        if depth == 0 {
            return Err(serde_json::Error::custom(
                "Workspace JSON is too deeply nested.",
            ));
        }
        match input.as_bytes().first() {
            Some(b'{') => serde_json::from_str::<BTreeMap<String, &RawValue>>(input)?
                .into_iter()
                .map(|(key, raw)| Ok((key, parse(raw.get(), depth - 1)?)))
                .collect::<serde_json::Result<_>>()
                .map(Value::Object),
            Some(b'[') => serde_json::from_str::<Vec<&RawValue>>(input)?
                .into_iter()
                .map(|raw| parse(raw.get(), depth - 1))
                .collect::<serde_json::Result<_>>()
                .map(Value::Array),
            Some(b'0'..=b'9' | b'-') if input.contains(['.', 'e', 'E']) => input
                .parse::<f64>()
                .ok()
                .and_then(Number::from_f64)
                .map(Value::Number)
                .ok_or_else(|| serde_json::Error::custom("Workspace JSON number is out of range.")),
            _ => serde_json::from_str(input),
        }
    }
    let raw: &RawValue = serde_json::from_str(input)?;
    parse(raw.get(), 64)
}

fn deserialize_json<'de, D>(deserializer: D) -> std::result::Result<Value, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = <&RawValue>::deserialize(deserializer)?;
    exact_json(raw.get()).map_err(serde::de::Error::custom)
}

pub fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub fn executable_digest() -> io::Result<String> {
    let mut executable = File::open(std::env::current_exe()?)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let bytes = executable.read(&mut buffer)?;
        if bytes == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buffer[..bytes]);
    }
}

pub fn token() -> Result<String> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| Error::internal("OS randomness unavailable."))?;
    Ok(hex(&bytes))
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("writing to String is infallible");
    }
    output
}

pub fn valid_token(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn read_owner(path: &Path) -> io::Result<String> {
    let file = File::open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if file.metadata()?.permissions().mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Owner token file must be private (chmod 600).",
            ));
        }
    }
    let mut secret = String::new();
    file.take(67).read_to_string(&mut secret)?;
    let secret = secret.trim_end_matches(['\r', '\n']);
    if !valid_token(secret) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Owner token file must contain 64 lowercase hexadecimal characters.",
        ));
    }
    Ok(digest(secret.as_bytes()))
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Editor,
    Viewer,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Review {
    #[default]
    Draft,
    #[serde(rename = "in_review")]
    Submitted,
    Approved,
    ChangesRequested,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Projection {
    Scenario,
    Report,
    Map,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Inputs {
    pub executable_sha256: String,
    pub city_snapshot_sha256: String,
    pub city_view_sha256: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Project {
    pub title: String,
    pub area: String,
    pub date: String,
    #[serde(deserialize_with = "deserialize_json")]
    pub city: Value,
    pub inputs: Inputs,
    pub grants: BTreeMap<String, Grant>,
    pub scenarios: Vec<Scenario>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Grant {
    pub role: Role,
    pub credential_sha256: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Scenario {
    pub title: String,
    pub revisions: Vec<Revision>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanInput {
    pub projects: Vec<usize>,
    pub width: u32,
    pub capital: u32,
    pub annual: u32,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Revision {
    pub plan: PlanInput,
    pub review: Review,
    pub comments: Vec<Comment>,
    pub job: Option<Job>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Comment {
    pub role: Role,
    pub member: String,
    pub text: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Job {
    Running,
    Interrupted,
    Failed {
        error: String,
    },
    Completed {
        #[serde(deserialize_with = "deserialize_json")]
        result: Value,
    },
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Share {
    pub project: usize,
    pub projection: Projection,
    #[serde(deserialize_with = "deserialize_json")]
    pub payload: Value,
    pub payload_sha256: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Ledger {
    version: u32,
    pub projects: Vec<Project>,
    pub shares: BTreeMap<String, Share>,
}

pub struct Store {
    path: PathBuf,
    _lock: File,
    pub ledger: Ledger,
}

impl Store {
    pub fn open(dir: &Path) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(dir)?;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Workspace directory must be private (chmod 700).",
                ));
            }
        }
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join("workspace.lock"))?;
        lock.try_lock().map_err(io::Error::from)?;
        let path = dir.join("workspace-v1.json");
        let mut ledger = match File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
                if bytes.len() as u64 > MAX_BYTES {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Workspace exceeds storage limit.",
                    ));
                }
                let ledger: Ledger = serde_json::from_slice(&bytes)?;
                if ledger.version != 1 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Unsupported workspace version.",
                    ));
                }
                ledger
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ledger {
                version: 1,
                projects: Vec::new(),
                shares: BTreeMap::new(),
            },
            Err(error) => return Err(error),
        };
        for project in &ledger.projects {
            for revision in project
                .scenarios
                .iter()
                .flat_map(|scenario| &scenario.revisions)
            {
                if let Some(Job::Completed { result }) = &revision.job {
                    super::export::verify(&project.inputs, &revision.plan, result).map_err(
                        |_| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Saved evaluation integrity check failed.",
                            )
                        },
                    )?;
                }
            }
        }
        if ledger.shares.values().any(|share| {
            digest(share.payload.to_string().as_bytes()) != share.payload_sha256
                || share.project >= ledger.projects.len()
        }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Saved share integrity check failed.",
            ));
        }
        for revision in ledger.projects.iter_mut().flat_map(|project| {
            project
                .scenarios
                .iter_mut()
                .flat_map(|scenario| &mut scenario.revisions)
        }) {
            if matches!(revision.job, Some(Job::Running)) {
                revision.job = Some(Job::Interrupted);
            }
        }
        // Existing running records represent interrupted work until the next
        // successful write. Reading the ledger remains possible at capacity.
        Ok(Self {
            path,
            _lock: lock,
            ledger,
        })
    }

    pub fn change<T>(&mut self, edit: impl FnOnce(&mut Ledger) -> Result<T>) -> Result<T> {
        let mut next = self.ledger.clone();
        let response = edit(&mut next)?;
        if let Err(error) = self.persist(&next) {
            if error.kind() == io::ErrorKind::StorageFull {
                return Err(Error::conflict(
                    "The workspace storage limit has been reached.",
                ));
            }
            eprintln!("Workspace save failed: {}", error.kind());
            return Err(Error::internal(
                "Workspace save failed. Check local storage.",
            ));
        }
        self.ledger = next;
        Ok(response)
    }

    fn persist(&self, ledger: &Ledger) -> io::Result<()> {
        let bytes = serde_json::to_vec(ledger)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        let dir = self.path.parent().expect("workspace path has a directory");
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist(&self.path)?;
        // The rename is the commit point. Keep memory in sync even when a
        // directory sync fails after the completed rename.
        if let Err(error) = File::open(dir).and_then(|dir| dir.sync_all()) {
            eprintln!("Workspace directory sync failed: {}", error.kind());
        }
        Ok(())
    }
}
