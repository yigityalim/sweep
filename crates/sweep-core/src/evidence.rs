use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Proven,
    Refuted,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub code: String,
    pub status: EvidenceStatus,
    pub detail: String,
}

impl Evidence {
    pub fn proven(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            status: EvidenceStatus::Proven,
            detail: detail.into(),
        }
    }

    pub fn refuted(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            status: EvidenceStatus::Refuted,
            detail: detail.into(),
        }
    }

    pub fn unknown(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            status: EvidenceStatus::Unknown,
            detail: detail.into(),
        }
    }
}
