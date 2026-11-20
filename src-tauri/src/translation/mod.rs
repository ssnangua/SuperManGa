mod providers;
mod types;

pub use providers::{translate_regions_batch, translate_text};
pub use types::{TranslationRequest, TranslationResponse, TranslationSettings};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateCommandArgs {
    pub text: String,
    pub settings: TranslationSettings,
    pub source_lang: Option<String>,
}

#[tauri::command]
pub async fn list_ai_models(base_url: String, api_key: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || providers::list_openai_compat_models(&base_url, &api_key))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn translate_text_cmd(args: TranslateCommandArgs) -> Result<TranslationResponse, String> {
    let req = TranslationRequest {
        text: args.text,
        settings: args.settings,
        source_lang: args.source_lang,
    };
    tauri::async_runtime::spawn_blocking(move || translate_text(req))
        .await
        .map_err(|err| err.to_string())?
}
