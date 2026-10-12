//! Offline checksums for the graph loaded by one entry point. This is not a
//! version solver or a checksum of every file in a dependency directory.
use crate::build_cache::sha256;
use crate::json::{self, Value};
use crate::project::{LoadedProgram, Manifest};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Component, Path};

const HEADER: &str = "bhasa-loaded-graph-v1";
const MAX_LOCK: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    entry: String,
    // Paths are relative to the entry manifest root, including sibling packages.
    records: BTreeMap<String, String>,
}

impl Snapshot {
    /// Hash bytes already consumed by the loader, including manifest text. Never
    /// reread source files or hash build caches. The root manifest is required.
    pub fn capture(loaded: &LoadedProgram, root: &Manifest) -> Result<Self, String> {
        if loaded.root_manifest_path.as_ref() != Some(&root.path) {
            return Err("snapshot root differs from the invocation's selected manifest".into());
        }
        let mut records = BTreeMap::new();
        for manifest in &loaded.manifests {
            let path = relative(&root.root, &manifest.path)?;
            if records.insert(path, manifest.checksum.clone()).is_some() {
                return Err("duplicate package identity in loaded graph".into());
            }
        }
        if !loaded
            .manifests
            .iter()
            .any(|manifest| manifest.path == root.path)
        {
            return Err("entry manifest is absent from loaded graph".into());
        }
        for file in &loaded.files {
            if records
                .insert(
                    relative(&root.root, &file.path)?,
                    sha256(file.source.as_bytes()),
                )
                .is_some()
            {
                return Err("duplicate file identity in loaded graph".into());
            }
        }
        let entry = relative(&root.root, &loaded.entry)?;
        if !records.contains_key(&entry) {
            return Err("loaded entry is absent from lock graph records".into());
        }
        let snapshot = Self { entry, records };
        if snapshot.records.len() > 2048 || snapshot.encode().len() > MAX_LOCK {
            return Err("loaded graph exceeds lock format limits".into());
        }
        Ok(snapshot)
    }

    pub fn encode(&self) -> String {
        let records = Value::Object(
            self.records
                .iter()
                .map(|(path, hash)| (path.clone(), Value::from(hash.clone())))
                .collect(),
        );
        format!(
            "{}\n",
            Value::object([
                ("format", Value::from(HEADER)),
                ("entry", Value::from(self.entry.clone())),
                ("files", records),
            ])
        )
    }

    pub fn decode(text: &str) -> Result<Self, String> {
        if text.len() > MAX_LOCK {
            return Err("bhasa.lock exceeds 1 MiB".into());
        }
        let value = json::parse(text)?;
        let Value::Object(object) = &value else {
            return Err("lock must be an object".into());
        };
        if object.len() != 3 || value.get("format").and_then(Value::as_str) != Some(HEADER) {
            return Err("unknown lock format or fields".into());
        }
        let entry = value
            .get("entry")
            .and_then(Value::as_str)
            .ok_or("missing lock entry")?;
        valid_path(entry)?;
        let Value::Object(files) = value.get("files").ok_or("missing lock files")? else {
            return Err("lock files must be an object".into());
        };
        if files.is_empty() || files.len() > 2048 {
            return Err("invalid lock graph size".into());
        }
        let mut records = BTreeMap::new();
        for (path, value) in files {
            valid_path(path)?;
            let hash = value.as_str().ok_or("checksum must be a string")?;
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(format!("invalid SHA-256 for {path:?}"));
            }
            records.insert(path.clone(), hash.to_owned());
        }
        if !records.contains_key(entry) {
            return Err("entry missing from lock files".into());
        }
        Ok(Self {
            entry: entry.to_owned(),
            records,
        })
    }

    /// Call only after validating the primary graph through the normal loader.
    /// Test sources may change freely below root/tests, but dependencies and
    /// production modules remain pinned by the primary entry lock.
    pub(crate) fn verify_analysis_graph(
        &self,
        loaded: &LoadedProgram,
        root: &Manifest,
    ) -> Result<(), String> {
        for manifest in &loaded.manifests {
            let path = relative(&root.root, &manifest.path)?;
            if self.records.get(&path) != Some(&manifest.checksum) {
                return Err(format!(
                    "test graph manifest is absent or changed in bhasa.lock: {path:?}"
                ));
            }
        }
        let tests = root.root.join("tests");
        for file in &loaded.files {
            if file.path.starts_with(&tests) {
                continue;
            }
            let path = relative(&root.root, &file.path)?;
            if self.records.get(&path) != Some(&sha256(file.source.as_bytes())) {
                return Err(format!(
                    "test graph source is absent or changed in bhasa.lock: {path:?}"
                ));
            }
        }
        Ok(())
    }

    pub fn verify(&self, actual: &Self) -> Result<(), String> {
        if self.entry != actual.entry {
            return Err("bhasa.lock was generated for a different entry point".into());
        }
        for (path, expected) in &self.records {
            match actual.records.get(path) {
                Some(hash) if hash == expected => {}
                Some(_) => return Err(format!("bhasa.lock checksum mismatch for {path:?}")),
                None => return Err(format!("bhasa.lock file is no longer loaded: {path:?}")),
            }
        }
        if let Some(path) = actual
            .records
            .keys()
            .find(|path| !self.records.contains_key(*path))
        {
            return Err(format!("bhasa.lock does not contain loaded file {path:?}"));
        }
        Ok(())
    }
}

/// An absent lock retains legacy behavior. A present lock is never repaired or
/// overwritten implicitly. Fail before Cargo or native compilation is invoked.
pub fn read_if_present(root: &Path) -> Result<Option<Snapshot>, String> {
    let path = root.join("bhasa.lock");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot inspect bhasa.lock: {error}")),
    };
    if !metadata.is_file() || metadata.len() > MAX_LOCK as u64 {
        return Err(
            "bhasa.lock must be a regular file of at most 1 MiB; symlinks are rejected".into(),
        );
    }
    let file = fs::File::open(&path).map_err(|error| format!("cannot read bhasa.lock: {error}"))?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("bhasa.lock is not a regular file".into());
    }
    let mut text = String::new();
    file.take(MAX_LOCK as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("cannot read bhasa.lock: {error}"))?;
    Snapshot::decode(&text).map(Some)
}

pub fn verify_if_present(loaded: &LoadedProgram, root: &Manifest) -> Result<(), String> {
    if let Some(expected) = read_if_present(&root.root)? {
        expected.verify(&Snapshot::capture(loaded, root)?)?;
    }
    Ok(())
}

/// Resolve the canonical manifest selected by the loader. Never infer package
/// authority from the canonical entry's location or dependency traversal.
pub fn root_manifest(loaded: &LoadedProgram) -> Option<&Manifest> {
    let path = loaded.root_manifest_path.as_ref()?;
    loaded
        .manifests
        .iter()
        .find(|manifest| &manifest.path == path)
}

fn valid_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains(['\0', '\\'])
        || path.starts_with('/')
        || path.split('/').any(|part| part.is_empty() || part == ".")
        || path.split('/').any(|part| part.contains(':'))
    {
        return Err("lock paths must be normalized portable relative paths".into());
    }
    // Parent segments may only precede normal components.
    let mut normal = false;
    for part in path.split('/') {
        if part == ".." {
            if normal {
                return Err("non-normal lock path".into());
            }
        } else {
            normal = true;
        }
    }
    if !normal {
        return Err("lock path must name a file".into());
    }
    Ok(())
}

fn relative(root: &Path, path: &Path) -> Result<String, String> {
    let root: Vec<_> = root.components().collect();
    let path: Vec<_> = path.components().collect();
    let common = root.iter().zip(&path).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return Err("lock cannot represent packages on different volumes".into());
    }
    let mut parts = vec!["..".to_owned(); root.len() - common];
    for component in &path[common..] {
        let Component::Normal(part) = component else {
            return Err("lock input paths must be canonical".into());
        };
        parts.push(
            part.to_str()
                .ok_or("lock input path must be UTF-8")?
                .to_owned(),
        );
    }
    let result = parts.join("/");
    valid_path(&result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        Snapshot {
            entry: "src/main.bh".into(),
            records: BTreeMap::from([
                ("src/main.bh".into(), sha256(b"def main() {}\n")),
                ("../math/bhasa.toml".into(), sha256(b"manifest")),
            ]),
        }
    }
    #[test]
    fn encoding_roundtrips_unicode_quotes_and_rejects_duplicate_fields() {
        let mut value = snapshot();
        value
            .records
            .insert("../math/src/λ\".bh".into(), sha256(b"source"));
        assert_eq!(Snapshot::decode(&value.encode()).unwrap(), value);
        assert!(Snapshot::decode(
            r#"{"format":"bhasa-loaded-graph-v1","entry":"x","entry":"y","files":{}}"#
        )
        .is_err());
        assert!(Snapshot::decode(&value.encode().replace("\"files\":", "\"unknown\":")).is_err());
    }
    #[test]
    fn verifies_content_membership_and_entry_without_repair() {
        let locked = snapshot();
        locked.verify(&snapshot()).unwrap();
        let mut changed = snapshot();
        changed
            .records
            .insert("src/main.bh".into(), sha256(b"changed"));
        assert!(locked
            .verify(&changed)
            .unwrap_err()
            .contains("checksum mismatch"));
        changed = snapshot();
        changed.records.remove("../math/bhasa.toml");
        assert!(locked.verify(&changed).is_err());
        changed = snapshot();
        changed.records.insert("src/new.bh".into(), sha256(b"new"));
        assert!(locked.verify(&changed).is_err());
        changed = snapshot();
        changed.entry = "other.bh".into();
        assert!(locked.verify(&changed).is_err());
    }
    #[test]
    fn normalized_relocatable_paths_and_checksum_validation() {
        assert_eq!(
            relative(Path::new("/work/app"), Path::new("/work/math/src/lib.bh")).unwrap(),
            "../math/src/lib.bh"
        );
        assert_eq!(
            relative(Path::new("/copy/app"), Path::new("/copy/math/src/lib.bh")).unwrap(),
            "../math/src/lib.bh"
        );
        for bad in ["", "/x", "a/../b", "a//b", "./a", "C:/x", "a\\b", ".."] {
            assert!(valid_path(bad).is_err(), "{bad}");
        }
        assert!(
            Snapshot::decode(&snapshot().encode().replace(&sha256(b"manifest"), "abc")).is_err()
        );
        assert!(Snapshot::decode(&"x".repeat(MAX_LOCK + 1)).is_err());
    }
}
