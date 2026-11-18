use super::*;
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::Component};

const LIMIT: u64 = 512 * 1024 * 1024;

fn err(e: impl fmt::Display) -> Error {
    Error::new(e.to_string())
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn check_digest(id: &str) -> Result<()> {
    if id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err(Error::new("expected a lowercase SHA-256 digest"))
    }
}

fn relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(Error::new(format!(
            "invalid relative input path {}",
            path.display()
        )));
    }
    Ok(())
}

fn immutable(directory: &Path, name: &str, content: &[u8]) -> Result<()> {
    fs::create_dir_all(directory).map_err(err)?;
    let destination = directory.join(name);
    let mut temp = tempfile::NamedTempFile::new_in(directory).map_err(err)?;
    temp.write_all(content)
        .and_then(|()| temp.as_file().sync_all())
        .map_err(err)?;
    match temp.persist_noclobber(&destination) {
        Ok(_) => Ok(()),
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_limited(&destination, content.len() as u64)? == content {
                Ok(())
            } else {
                Err(Error::new(format!(
                    "immutable object conflict: {}",
                    destination.display()
                )))
            }
        }
        Err(e) => Err(err(e)),
    }
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let e = |error| Error::new(format!("{}: {error}", path.display()));
    let meta = fs::symlink_metadata(path).map_err(e)?;
    if !meta.is_file() || meta.len() > limit {
        return Err(Error::new(format!(
            "{} must be a regular file within its size limit",
            path.display()
        )));
    }
    let mut content = Vec::new();
    File::open(path)
        .map_err(e)?
        .take(limit + 1)
        .read_to_end(&mut content)
        .map_err(e)?;
    if content.len() as u64 > limit {
        return Err(Error::new("object exceeds its size limit"));
    }
    Ok(content)
}

fn inputs(root: &Path, input: &Input) -> Result<Vec<PathBuf>> {
    match input {
        Input::Gtfs {
            directory,
            realtime,
        } => {
            if !resolved(root, directory)?.is_dir() {
                return Err(Error::new("GTFS directory must be a directory"));
            }
            let mut paths = BTreeSet::new();
            for table in super::super::gtfs::TABLE_NAMES {
                let path = directory.join(table);
                if super::super::tables::optional_exists(root, &path)? {
                    paths.insert(path);
                }
            }
            if let Some(rt) = realtime {
                paths.insert(rt.path.clone());
            }
            Ok(paths.into_iter().collect())
        }
        Input::Osm { path }
        | Input::CensusPopulation { path, .. }
        | Input::Lodes { path, .. }
        | Input::Places { path }
        | Input::Dot { path, .. }
        | Input::Crashes { path, .. }
        | Input::Emissions { path, .. }
        | Input::Custom { path, .. } => Ok(vec![path.clone()]),
    }
}

fn stage(root: &Path, path: &Path, content: &[u8]) -> Result<()> {
    relative(path)?;
    let location = root.join(path);
    fs::create_dir_all(
        location
            .parent()
            .ok_or_else(|| Error::new("missing staging parent"))?,
    )
    .map_err(err)?;
    fs::write(location, content).map_err(err)
}

fn add_size(total: &mut u64, count: u64) -> Result<()> {
    *total += count;
    if *total > LIMIT {
        return Err(Error::new("recipe raw inputs exceed 512 MiB total"));
    }
    Ok(())
}

/// Archive exactly the bytes consumed by the connectors and create an immutable snapshot.
/// Paths in each request are relative to `root`. Use a private, trusted `store`.
/// Returns the SHA-256 of `store/snapshots/<id>.json`.
pub fn build(recipe: Recipe, root: &Path, store: &Path) -> Result<String> {
    recipe.validate()?;
    let mut total = 0;
    let mut raw = vec![];
    let mut sources = vec![];
    for (input, entry) in recipe.inputs.iter().enumerate() {
        let staging = tempfile::tempdir().map_err(err)?;
        for path in inputs(root, &entry.request.input)? {
            let content = bytes(root, &path)?;
            let size = content.len() as u64;
            add_size(&mut total, size)?;
            let sha256 = digest(&content);
            immutable(&store.join("raw"), &sha256, &content)?;
            stage(staging.path(), &path, &content)?;
            raw.push(RawFile {
                input,
                path,
                sha256,
                bytes: size,
            });
        }
        sources.push(inspect(&entry.request, staging.path(), &recipe.as_of)?);
    }
    let data = encode(&derive(recipe, raw, sources)?)?;
    let id = digest(&data);
    immutable(&store.join("snapshots"), &format!("{id}.json"), &data)?;
    Ok(id)
}

fn encode(snapshot: &Snapshot) -> Result<Vec<u8>> {
    let content = serde_json::to_vec(snapshot).map_err(err)?;
    if content.len() as u64 > LIMIT {
        return Err(Error::new("derived snapshot exceeds 512 MiB"));
    }
    Ok(content)
}

/// Read and hash-check a snapshot. Raw objects are checked separately by `rebuild`.
pub fn load(store: &Path, id: &str) -> Result<Snapshot> {
    check_digest(id)?;
    let data = read_limited(&store.join("snapshots").join(format!("{id}.json")), LIMIT)?;
    if digest(&data) != id {
        return Err(Error::new("snapshot SHA-256 mismatch"));
    }
    let snapshot: Snapshot = serde_json::from_slice(&data).map_err(err)?;
    if snapshot.schema_version != SNAPSHOT_SCHEMA
        || snapshot.processing_version != PROCESSING_VERSION
    {
        return Err(Error::new(
            "unsupported snapshot schema or processing version; use the matching release",
        ));
    }
    snapshot.recipe.validate()?;
    if snapshot.layers.len() != snapshot.recipe.inputs.len()
        || snapshot
            .layers
            .iter()
            .enumerate()
            .any(|(i, layer)| layer.input != i)
    {
        return Err(Error::new("snapshot layers do not match recipe"));
    }
    Ok(snapshot)
}

/// Replay from archived inputs only, verifying raw hashes and exact derived bytes.
/// Never repairs or overwrites the original objects.
pub fn rebuild(store: &Path, id: &str) -> Result<()> {
    let Snapshot { recipe, raw, .. } = load(store, id)?;
    let mut total = 0;
    let mut seen = BTreeSet::new();
    for file in &raw {
        check_digest(&file.sha256)?;
        relative(&file.path)?;
        if file.input >= recipe.inputs.len()
            || file.bytes > MAX_INPUT_BYTES
            || !seen.insert((file.input, &file.path))
        {
            return Err(Error::new("invalid or duplicate raw manifest entry"));
        }
        add_size(&mut total, file.bytes)?;
    }
    let mut sources = vec![];
    for (index, entry) in recipe.inputs.iter().enumerate() {
        let staging = tempfile::tempdir().map_err(err)?;
        for file in raw.iter().filter(|file| file.input == index) {
            let content = read_limited(&store.join("raw").join(&file.sha256), file.bytes)?;
            if content.len() as u64 != file.bytes || digest(&content) != file.sha256 {
                return Err(Error::new(format!(
                    "raw SHA-256 or size mismatch for {} ({})",
                    file.path.display(),
                    file.sha256
                )));
            }
            stage(staging.path(), &file.path, &content)?;
        }
        sources.push(inspect(&entry.request, staging.path(), &recipe.as_of)?);
    }
    if digest(&encode(&derive(recipe, raw, sources)?)?) != id {
        return Err(Error::new("rebuilt snapshot SHA-256 mismatch"));
    }
    Ok(())
}
