use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationAiEndpoint {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_max_tokens() -> u32 {
    2048
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationAiProfile {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub endpoint: TranslationAiEndpoint,
}

impl Default for TranslationAiEndpoint {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
            max_tokens: 2048,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationSettings {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_target")]
    pub target_lang: String,
    #[serde(default = "default_source")]
    pub source_lang: String,
    #[serde(default = "default_ocr_engine")]
    pub ocr_engine: String,
    /// Primary text detector: off | ctd | default | yolo | saber_yolo.
    #[serde(default = "default_text_detector")]
    pub text_detector: String,
    /// Merge YSGYolo boxes into primary detector output.
    #[serde(default)]
    pub enable_aux_yolo: bool,
    /// Split over-merged blocks using SaberYOLO references (Saber default True).
    #[serde(default = "default_true")]
    pub enable_saber_yolo_refine: bool,
    #[serde(default = "default_true")]
    pub show_original: bool,
    #[serde(default)]
    pub deepl_api_key: String,
    #[serde(default)]
    pub ai_profiles: Vec<TranslationAiProfile>,
    #[serde(default)]
    pub active_ai_profile_id: String,
    #[serde(default)]
    pub ai_base_url: String,
    #[serde(default)]
    pub ai_api_key: String,
    #[serde(default)]
    pub ai_model: String,
    #[serde(default = "default_max_tokens")]
    pub ai_max_tokens: u32,
    #[serde(default)]
    pub baidu_app_id: String,
    #[serde(default)]
    pub baidu_secret: String,
    #[serde(default)]
    pub youdao_app_key: String,
    #[serde(default)]
    pub youdao_app_secret: String,
    #[serde(default)]
    pub tencent_secret_id: String,
    #[serde(default)]
    pub tencent_secret_key: String,
    #[serde(default)]
    pub tencent_region: String,
    #[serde(default)]
    pub volc_access_key_id: String,
    #[serde(default)]
    pub volc_secret_key: String,
    #[serde(default)]
    pub volc_region: String,
    #[serde(default)]
    pub aliyun_access_key_id: String,
    #[serde(default)]
    pub aliyun_access_key_secret: String,
    /// Caiyun Lingocloud token.
    #[serde(default)]
    pub caiyun_token: String,
}

fn default_provider() -> String {
    "microsoft".into()
}
fn default_target() -> String {
    "zh-CN".into()
}
fn default_source() -> String {
    "ja".into()
}
fn default_ocr_engine() -> String {
    "windows".into()
}
fn default_text_detector() -> String {
    "default".into()
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone)]
pub struct TranslationRequest {
    pub text: String,
    pub settings: TranslationSettings,
    pub source_lang: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum TranslationResponse {
    Ok {
        ok: bool,
        translated: String,
        provider: String,
    },
    Err {
        ok: bool,
        message: String,
    },
}

impl TranslationResponse {
    pub fn ok(translated: String, provider: String) -> Self {
        Self::Ok {
            ok: true,
            translated,
            provider,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self::Err {
            ok: false,
            message: message.into(),
        }
    }
}
