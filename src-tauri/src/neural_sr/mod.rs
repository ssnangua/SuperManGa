pub mod cache;
pub mod cuda;
pub mod download;
pub mod infer;
pub mod models;
pub mod ort_env;
pub mod results;
pub mod runtime;

/// Progress event payload for runtime/model downloads.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub kind: String,
    pub id: String,
    pub done: u64,
    pub total: Option<u64>,
    pub percent: Option<u32>,
}
