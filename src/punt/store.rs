use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::PuntError;
use crate::punt::continuation::{canonical_bytes, ContinuationObject};

pub trait ContinuationStore {
    fn prepare(&mut self, object: &ContinuationObject) -> Result<(), PuntError>;
    fn load_prepared(
        &self,
        drive_id: &str,
        generation: u64,
    ) -> Result<ContinuationObject, PuntError>;
    fn commit(&mut self, drive_id: &str, generation: u64) -> Result<(), PuntError>;
    fn load_committed(
        &self,
        drive_id: &str,
        generation: u64,
    ) -> Result<ContinuationObject, PuntError>;
    fn latest_committed(&self, drive_id: &str) -> Result<Option<ContinuationObject>, PuntError>;
}

#[derive(Clone, Debug, Default)]
pub struct MemoryStore {
    prepared: BTreeMap<(String, u64), ContinuationObject>,
    committed: BTreeMap<(String, u64), ContinuationObject>,
}

impl MemoryStore {
    pub fn prepared_count(&self) -> usize {
        self.prepared.len()
    }

    pub fn committed_count_for(&self, drive_id: &str) -> usize {
        self.committed
            .keys()
            .filter(|(id, _)| id == drive_id)
            .count()
    }
}

impl ContinuationStore for MemoryStore {
    fn prepare(&mut self, object: &ContinuationObject) -> Result<(), PuntError> {
        self.prepared.insert(
            (object.identity.drive_id.clone(), object.identity.generation),
            object.clone(),
        );
        Ok(())
    }

    fn load_prepared(
        &self,
        drive_id: &str,
        generation: u64,
    ) -> Result<ContinuationObject, PuntError> {
        self.prepared
            .get(&(drive_id.to_string(), generation))
            .cloned()
            .ok_or(PuntError::NotCommitted)
    }

    fn commit(&mut self, drive_id: &str, generation: u64) -> Result<(), PuntError> {
        let object = self.load_prepared(drive_id, generation)?;
        self.committed
            .insert((drive_id.to_string(), generation), object);
        Ok(())
    }

    fn load_committed(
        &self,
        drive_id: &str,
        generation: u64,
    ) -> Result<ContinuationObject, PuntError> {
        self.committed
            .get(&(drive_id.to_string(), generation))
            .cloned()
            .ok_or(PuntError::NotCommitted)
    }

    fn latest_committed(&self, drive_id: &str) -> Result<Option<ContinuationObject>, PuntError> {
        Ok(self
            .committed
            .iter()
            .filter(|((id, _), _)| id == drive_id)
            .max_by_key(|((_, generation), _)| *generation)
            .map(|(_, object)| object.clone()))
    }
}

/// Store that returns a mutated prepared object so replay validation fails closed.
#[derive(Debug)]
pub struct MutatingReadStore {
    inner: MemoryStore,
}

impl Default for MutatingReadStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MutatingReadStore {
    pub fn new() -> Self {
        Self {
            inner: MemoryStore::default(),
        }
    }

    pub fn committed_len(&self) -> usize {
        self.inner.committed.len()
    }
}

impl ContinuationStore for MutatingReadStore {
    fn prepare(&mut self, object: &ContinuationObject) -> Result<(), PuntError> {
        self.inner.prepare(object)
    }

    fn load_prepared(
        &self,
        drive_id: &str,
        generation: u64,
    ) -> Result<ContinuationObject, PuntError> {
        let mut object = self.inner.load_prepared(drive_id, generation)?;
        if let Some(fact) = object.evidence.observations.first_mut() {
            fact.value.push('x');
        } else if let Some(artifact) = object.evidence.artifacts.first_mut() {
            artifact.body.push('x');
        } else {
            object.position.unresolved.push("tampered".to_string());
        }
        Ok(object)
    }

    fn commit(&mut self, drive_id: &str, generation: u64) -> Result<(), PuntError> {
        self.inner.commit(drive_id, generation)
    }

    fn load_committed(
        &self,
        drive_id: &str,
        generation: u64,
    ) -> Result<ContinuationObject, PuntError> {
        self.inner.load_committed(drive_id, generation)
    }

    fn latest_committed(&self, drive_id: &str) -> Result<Option<ContinuationObject>, PuntError> {
        self.inner.latest_committed(drive_id)
    }
}

#[derive(Debug)]
pub struct FsStore {
    root: PathBuf,
}

impl FsStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn dir(&self, drive_id: &str) -> PathBuf {
        self.root.join(drive_id)
    }

    fn prepared_path(&self, drive_id: &str, generation: u64) -> PathBuf {
        self.dir(drive_id)
            .join(format!("{generation}.prepared.json"))
    }

    fn committed_path(&self, drive_id: &str, generation: u64) -> PathBuf {
        self.dir(drive_id)
            .join(format!("{generation}.committed.json"))
    }

    fn marker_path(&self, drive_id: &str, generation: u64) -> PathBuf {
        self.dir(drive_id).join(format!("{generation}.COMMIT"))
    }
}

impl ContinuationStore for FsStore {
    fn prepare(&mut self, object: &ContinuationObject) -> Result<(), PuntError> {
        let directory = self.dir(&object.identity.drive_id);
        fs::create_dir_all(&directory).map_err(|error| PuntError::Store(error.to_string()))?;
        let bytes = canonical_bytes(object)?;
        write_atomic(
            &self.prepared_path(&object.identity.drive_id, object.identity.generation),
            &bytes,
        )
    }

    fn load_prepared(
        &self,
        drive_id: &str,
        generation: u64,
    ) -> Result<ContinuationObject, PuntError> {
        read_object(&self.prepared_path(drive_id, generation))
    }

    fn commit(&mut self, drive_id: &str, generation: u64) -> Result<(), PuntError> {
        let object = self.load_prepared(drive_id, generation)?;
        let bytes = canonical_bytes(&object)?;
        write_atomic(&self.committed_path(drive_id, generation), &bytes)?;
        write_atomic(&self.marker_path(drive_id, generation), b"committed\n")?;
        Ok(())
    }

    fn load_committed(
        &self,
        drive_id: &str,
        generation: u64,
    ) -> Result<ContinuationObject, PuntError> {
        if !self.marker_path(drive_id, generation).is_file() {
            return Err(PuntError::NotCommitted);
        }
        read_object(&self.committed_path(drive_id, generation))
    }

    fn latest_committed(&self, drive_id: &str) -> Result<Option<ContinuationObject>, PuntError> {
        let directory = self.dir(drive_id);
        if !directory.is_dir() {
            return Ok(None);
        }
        let mut best: Option<u64> = None;
        for entry in
            fs::read_dir(&directory).map_err(|error| PuntError::Store(error.to_string()))?
        {
            let entry = entry.map_err(|error| PuntError::Store(error.to_string()))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if let Some(generation) = name.strip_suffix(".COMMIT") {
                if let Ok(generation) = generation.parse::<u64>() {
                    best = Some(best.map_or(generation, |current| current.max(generation)));
                }
            }
        }
        match best {
            Some(generation) => self.load_committed(drive_id, generation).map(Some),
            None => Ok(None),
        }
    }
}

fn read_object(path: &Path) -> Result<ContinuationObject, PuntError> {
    let bytes = fs::read(path).map_err(|_| PuntError::NotCommitted)?;
    serde_json::from_slice(&bytes).map_err(|error| PuntError::Store(error.to_string()))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), PuntError> {
    let directory = path
        .parent()
        .ok_or_else(|| PuntError::Store("missing parent".to_string()))?;
    let mut temporary = directory.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("object")
    ));
    // Unique enough for a single writer.
    temporary.set_extension(format!("tmp-{}", std::process::id()));
    {
        let mut file =
            fs::File::create(&temporary).map_err(|error| PuntError::Store(error.to_string()))?;
        file.write_all(bytes)
            .map_err(|error| PuntError::Store(error.to_string()))?;
        file.sync_all()
            .map_err(|error| PuntError::Store(error.to_string()))?;
    }
    fs::rename(&temporary, path).map_err(|error| PuntError::Store(error.to_string()))?;
    Ok(())
}
