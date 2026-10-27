//! A committed continuation written for a later process.
//!
//! The file is the canonical package. The receiver route stays in the resume
//! request that the fresh process supplies.

use std::fs;
use std::io::Write;
use std::path::Path;

use crate::punt::continuation::{
    canonical_bytes, leaks_competitive_metadata, validate, ContinuationObject,
};
use crate::sim::runner::{committed_package, resume_committed, Campaign, Fixture, SimError};

pub fn write_handoff(path: &Path, bytes: &[u8]) -> Result<(), SimError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| crate::error::PuntError::Store(error.to_string()))?;
        }
    }
    let temporary = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&temporary)
            .map_err(|error| crate::error::PuntError::Store(error.to_string()))?;
        file.write_all(bytes)
            .map_err(|error| crate::error::PuntError::Store(error.to_string()))?;
        file.sync_all()
            .map_err(|error| crate::error::PuntError::Store(error.to_string()))?;
    }
    fs::rename(&temporary, path)
        .map_err(|error| crate::error::PuntError::Store(error.to_string()))?;
    Ok(())
}

pub fn read_handoff(path: &Path) -> Result<ContinuationObject, SimError> {
    let bytes =
        fs::read(path).map_err(|error| crate::error::PuntError::Store(error.to_string()))?;
    if leaks_competitive_metadata(&bytes) {
        return Err(SimError::Fixture("leak"));
    }
    let object: ContinuationObject = serde_json::from_slice(&bytes)
        .map_err(|error| crate::error::PuntError::Store(error.to_string()))?;
    validate(&object)?;
    let canonical = canonical_bytes(&object)?;
    if canonical != bytes {
        return Err(SimError::Fixture("canonical"));
    }
    Ok(object)
}

pub fn export_handoff(campaign: &Campaign, path: &Path) -> Result<(), SimError> {
    let bytes = committed_package(campaign)?;
    write_handoff(path, &bytes)
}

pub fn resume_handoff(fixture: &Fixture, path: &Path) -> Result<Campaign, SimError> {
    let object = read_handoff(path)?;
    resume_committed(fixture, &object)
}
