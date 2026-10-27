use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::field::{ClaimKind, FieldSnapshot};
use crate::error::{LedgerError, SealError};
use crate::security::{open, seal, SealedBlob};

/// Projection of a field onto Padagonia's epistemic vocabulary.
///
/// Confirmed conditions become observations. Claims that lack independent
/// confirmation stay assertions. Contradictions are kept as edges. Nothing in
/// this projection picks a winner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedClaim {
    pub id: String,
    pub kind: String,
    pub actor: String,
    pub authority_label: String,
    pub authority_rank: u32,
    pub subject: String,
    pub predicate: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedEvidence {
    pub id: String,
    pub supports: String,
    pub locator: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedContradiction {
    pub id: String,
    pub left: String,
    pub right: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedCorroboration {
    pub id: String,
    pub source: String,
    pub target: String,
    pub independent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedSupersession {
    pub id: String,
    pub winner: String,
    pub loser: String,
    pub operator: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerProjection {
    pub claims: Vec<ProjectedClaim>,
    pub evidence: Vec<ProjectedEvidence>,
    pub contradictions: Vec<ProjectedContradiction>,
    pub corroborations: Vec<ProjectedCorroboration>,
    pub supersessions: Vec<ProjectedSupersession>,
}

pub fn project(field: &FieldSnapshot) -> LedgerProjection {
    let mut claims = Vec::new();
    for fact in field.verified_facts.iter().chain(field.assertions.iter()) {
        claims.push(ProjectedClaim {
            id: fact.claim_id.as_str().to_string(),
            kind: match fact.kind {
                ClaimKind::Observation => "observation",
                ClaimKind::Assertion => "assertion",
            }
            .to_string(),
            actor: fact.authority.clone(),
            authority_label: fact.authority.clone(),
            authority_rank: fact.authority_rank,
            subject: fact.subject.clone(),
            predicate: fact.predicate.clone(),
            value: fact.value.clone(),
        });
    }
    let mut evidence = Vec::new();
    for artifact in &field.artifacts {
        evidence.push(ProjectedEvidence {
            id: format!("ev-{}", artifact.artifact_id.as_str()),
            supports: artifact.artifact_id.as_str().to_string(),
            locator: format!("artifact:{}", artifact.digest.as_str()),
        });
    }
    for test in &field.tests {
        evidence.push(ProjectedEvidence {
            id: format!("ev-{}", test.test_id),
            supports: test.test_id.clone(),
            locator: format!("test:{}", test.digest.as_str()),
        });
    }
    LedgerProjection {
        claims,
        evidence,
        contradictions: field
            .contradictions
            .iter()
            .map(|item| ProjectedContradiction {
                id: item.id.clone(),
                left: item.left.as_str().to_string(),
                right: item.right.as_str().to_string(),
            })
            .collect(),
        corroborations: field
            .corroborations
            .iter()
            .map(|item| ProjectedCorroboration {
                id: item.id.clone(),
                source: item.source.as_str().to_string(),
                target: item.target.as_str().to_string(),
                independent: item.independent,
            })
            .collect(),
        supersessions: field
            .supersessions
            .iter()
            .map(|item| ProjectedSupersession {
                id: item.id.clone(),
                winner: item.winner.as_str().to_string(),
                loser: item.loser.as_str().to_string(),
                operator: item.operator.clone(),
            })
            .collect(),
    }
}

/// Append-only field journal.
///
/// Record kinds follow Padagonia: observation, assertion, evidence,
/// contradiction, corroboration, and supersession. The padagonia repository is
/// a networked store, so this journal is the in-process ledger Lastdown commits
/// when a punt seals. Recording a supersession keeps both claims. This journal
/// does not choose a winner.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldLedger {
    entries: Vec<LedgerEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub sequence: u64,
    pub field_id: String,
    pub claims: Vec<ProjectedClaim>,
    pub evidence: Vec<ProjectedEvidence>,
    pub contradictions: Vec<ProjectedContradiction>,
    pub corroborations: Vec<ProjectedCorroboration>,
    pub supersessions: Vec<ProjectedSupersession>,
}

impl FieldLedger {
    pub fn record(&mut self, field: &FieldSnapshot) -> u64 {
        let projected = project(field);
        let sequence = self.entries.len() as u64 + 1;
        self.entries.push(LedgerEntry {
            sequence,
            field_id: field.field_id.as_str().to_string(),
            claims: projected.claims,
            evidence: projected.evidence,
            contradictions: projected.contradictions,
            corroborations: projected.corroborations,
            supersessions: projected.supersessions,
        });
        sequence
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    pub fn revise(&mut self, sequence: u64) -> Result<(), LedgerError> {
        if self.entries.iter().any(|entry| entry.sequence == sequence) {
            Err(LedgerError::Immutable)
        } else {
            Err(LedgerError::Missing)
        }
    }

    /// Keep the newest generations. The latest entry stays.
    pub fn apply_retention(&mut self, keep_generations: usize) {
        let keep = keep_generations.max(1);
        if self.entries.len() > keep {
            let drop_count = self.entries.len() - keep;
            self.entries.drain(0..drop_count);
        }
    }

    pub fn persist_sealed(&self, directory: &Path, key: &[u8]) -> Result<(), LedgerError> {
        fs::create_dir_all(directory).map_err(|error| LedgerError::Io(error.to_string()))?;
        let plain = serde_json::to_vec(self).map_err(|error| LedgerError::Io(error.to_string()))?;
        let mut nonce = [0u8; 16];
        let digest = crate::core::field::Digest::of(&plain);
        let digest_bytes = digest.as_str().as_bytes();
        for (index, byte) in nonce.iter_mut().enumerate() {
            *byte = digest_bytes[index];
        }
        let sealed = seal(key, &plain, &nonce);
        let bytes =
            serde_json::to_vec(&sealed).map_err(|error| LedgerError::Io(error.to_string()))?;
        let path = directory.join("journal.sealed.json");
        write_atomic(&path, &bytes)?;
        fs::write(directory.join("COMMIT"), b"1")
            .map_err(|error| LedgerError::Io(error.to_string()))?;
        Ok(())
    }

    pub fn load_sealed(directory: &Path, key: &[u8]) -> Result<Self, LedgerError> {
        let marker = directory.join("COMMIT");
        if !marker.exists() {
            return Err(LedgerError::NotCommitted);
        }
        let bytes = fs::read(directory.join("journal.sealed.json"))
            .map_err(|error| LedgerError::Io(error.to_string()))?;
        let sealed: SealedBlob =
            serde_json::from_slice(&bytes).map_err(|error| LedgerError::Io(error.to_string()))?;
        let plain = open(key, &sealed).map_err(|error| match error {
            SealError::Authentication | SealError::Encoding => LedgerError::Seal,
        })?;
        serde_json::from_slice(&plain).map_err(|error| LedgerError::Io(error.to_string()))
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), LedgerError> {
    let directory = path
        .parent()
        .ok_or_else(|| LedgerError::Io("missing parent".to_string()))?;
    let mut temporary = directory.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("journal"),
        std::process::id()
    ));
    if temporary.extension().is_some() {
        temporary.set_extension(format!("tmp-{}", std::process::id()));
    }
    {
        let mut file =
            fs::File::create(&temporary).map_err(|error| LedgerError::Io(error.to_string()))?;
        file.write_all(bytes)
            .map_err(|error| LedgerError::Io(error.to_string()))?;
        file.sync_all()
            .map_err(|error| LedgerError::Io(error.to_string()))?;
    }
    fs::rename(&temporary, path).map_err(|error| LedgerError::Io(error.to_string()))?;
    Ok(())
}
