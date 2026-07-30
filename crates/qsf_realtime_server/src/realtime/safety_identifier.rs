//! Shared OpenAI safety identifier derivation for realtime requests.

use sha2::{Digest, Sha256};

pub(crate) const OPENAI_SAFETY_IDENTIFIER_HEADER: &str = "OpenAI-Safety-Identifier";

pub(crate) fn hash_session_id(session_id: &str) -> String {
    let digest = Sha256::digest(session_id.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
