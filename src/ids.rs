use serde::{Deserialize, Serialize};

use crate::error::IdError;

macro_rules! id_ty {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                let value = value.into();
                if value.is_empty() || value.chars().any(|ch| ch.is_whitespace()) {
                    return Err(IdError::Empty);
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

id_ty!(DriveId);
id_ty!(FieldId);
id_ty!(GateId);
id_ty!(ArtifactId);
id_ty!(ClaimId);
id_ty!(EffectId);
id_ty!(RouteId);
