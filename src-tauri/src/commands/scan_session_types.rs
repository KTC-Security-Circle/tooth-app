use serde::{Deserialize, Serialize};

use crate::state::scan_session::{ScanSessionManifest, ScanSessionSlot, SessionStatus};
use crate::state::scan_slot::SlotStatus;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartScanSessionRequest {
    pub session_id: String,
    pub input_dirs: Vec<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRequest {
    pub session_id: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryScanSessionRequest {
    pub session_id: String,
    pub slot: u8,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSessionResult {
    pub session: ScanSessionResponse,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSessionResponse {
    pub version: u32,
    pub session_id: String,
    pub status: SessionStatus,
    pub current_slot: Option<u8>,
    pub slots: Vec<ScanSessionSlotResponse>,
    pub reason: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSessionSlotResponse {
    pub slot: u8,
    pub input_dir: String,
    pub status: Option<SlotStatus>,
    pub reason: Option<String>,
}

impl From<ScanSessionManifest> for ScanSessionResult {
    fn from(session: ScanSessionManifest) -> Self {
        Self {
            session: ScanSessionResponse {
                version: session.version,
                session_id: session.session_id,
                status: session.status,
                current_slot: session.current_slot,
                slots: session
                    .slots
                    .into_iter()
                    .map(|slot: ScanSessionSlot| ScanSessionSlotResponse {
                        slot: slot.slot,
                        input_dir: slot.input_dir,
                        status: slot.status,
                        reason: slot.reason,
                    })
                    .collect(),
                reason: session.reason,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_uses_camel_case_without_changing_manifest_format() {
        let manifest = ScanSessionManifest {
            version: 1,
            session_id: "session".into(),
            status: SessionStatus::Running,
            current_slot: Some(2),
            slots: vec![ScanSessionSlot {
                slot: 2,
                input_dir: "/scan".into(),
                status: None,
                reason: None,
            }],
            reason: None,
        };
        let response = serde_json::to_value(ScanSessionResult::from(manifest.clone()))
            .expect("response should serialize");
        assert!(response["session"]["sessionId"].is_string());
        assert!(response["session"]["currentSlot"].is_number());
        assert!(
            serde_json::to_value(manifest).expect("manifest should serialize")["session_id"]
                .is_string()
        );
    }
}
