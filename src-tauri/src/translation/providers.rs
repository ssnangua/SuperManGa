use super::types::{TranslationRequest, TranslationResponse, TranslationSettings};

/// Stable codes for the frontend (`formatTranslationError`). Keep ASCII; no UI locale text here.
fn tr_need(code: &str) -> String {
    code.into()
}

fn tr_empty(provider: &str) -> String {
    format!("TRANSLATE_EMPTY:{provider}")
}

/// Prefer OpenAI / cloud JSON `error.message` (etc.) over raw body for toast detail.
fn summarize_http_detail(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        for path in [
            "/error/message",
            "/message",
            "/Response/Error/Message",
            "/ResponseMetadata/Error/Message",
        ] {
            if let Some(msg) = v
                .pointer(path)
                .and_then(|x| x.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                return msg.chars().take(200).collect();
            }
        }
    }
    trimmed.chars().take(200).collect()
}

fn tr_http(provider: &str, status: u16, detail: &str) -> String {
    let d = summarize_http_detail(detail);
    if d.is_empty() {
        format!("TRANSLATE_HTTP:{provider}:{status}")
    } else {
        format!("TRANSLATE_HTTP:{provider}:{status}|{d}")
    }
}

/// ureq treats non-2xx as `Error::Status`; still return status + body so callers can surface API text.
fn ureq_status_body(result: Result<ureq::Response, ureq::Error>) -> Result<(u16, String), String> {
    match result {
        Ok(res) => {
            let status = res.status();
            let text = res.into_string().map_err(|err| err.to_string())?;
            Ok((status, text))
        }
        Err(ureq::Error::Status(code, res)) => {
            let text = res.into_string().unwrap_or_default();
            Ok((code, text))
        }
        Err(err) => Err(err.to_string()),
    }
}

fn map_lang(ui_lang: &str, provider: &str) -> String {
    let code = ui_lang.trim();
    let short = code.split('-').next().unwrap_or(code).to_lowercase();
    match provider {
        "microsoft" => {
            if code == "zh-CN" || code == "zh" {
                "zh-Hans".into()
            } else if code == "zh-TW" {
                "zh-Hant".into()
            } else {
                code.to_string()
            }
        }
        "google" | "yandex" | "ai" => {
            if short == "zh" {
                if code == "zh-TW" {
                    "zh-TW".into()
                } else {
                    "zh-CN".into()
                }
            } else {
                short
            }
        }
        "deepl" => {
            if code == "zh-TW" {
                "ZH-HANT".into()
            } else if short == "zh" {
                "ZH".into()
            } else if code.eq_ignore_ascii_case("pt-BR") {
                "PT-BR".into()
            } else if short == "pt" {
                "PT-PT".into()
            } else if code.eq_ignore_ascii_case("en-GB") {
                "EN-GB".into()
            } else if code.eq_ignore_ascii_case("en-US") {
                "EN-US".into()
            } else {
                short.to_uppercase()
            }
        }
        "baidu" => match (code, short.as_str()) {
            ("zh-TW", _) => "cht".into(),
            (_, "zh") => "zh".into(),
            (_, "ja") => "jp".into(),
            (_, "ko") => "kor".into(),
            (_, "fr") => "fra".into(),
            (_, "es") => "spa".into(),
            (_, "ar") => "ara".into(),
            (_, "vi") => "vie".into(),
            (_, "da") => "dan".into(),
            (_, "fi") => "fin".into(),
            (_, "sv") => "swe".into(),
            (_, "ro") => "rom".into(),
            (_, "bg") => "bul".into(),
            (_, "et") => "est".into(),
            (_, "lt") => "lit".into(),
            (_, "lv") => "lav".into(),
            (_, "uk") => "ukr".into(),
            (_, "ms") => "may".into(),
            (_, "fil") => "fil".into(),
            (_, "no") => "nor".into(),
            _ => short,
        },
        "youdao" => match (code, short.as_str()) {
            ("zh-TW", _) => "zh-CHT".into(),
            (_, "zh") => "zh-CHS".into(),
            _ => short,
        },
        "tencent" | "volcengine" | "aliyun" => {
            if code == "zh-TW" {
                "zh-TW".into()
            } else if short == "zh" {
                "zh".into()
            } else {
                short
            }
        }
        _ => short,
    }
}

fn http_post_json(url: &str, body: &str, headers: &[(&str, &str)]) -> Result<(u16, String), String> {
    let mut req = ureq::post(url).set("User-Agent", "SuperManGa/0.1");
    for (k, v) in headers {
        req = req.set(k, v);
    }
    ureq_status_body(req.send_string(body))
}

pub fn list_openai_compat_models(base_url: &str, api_key: &str) -> Result<Vec<String>, String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(tr_need("TRANSLATE_NEED_URL"));
    }
    let url = format!("{base}/models");
    let mut req = ureq::get(&url).set("User-Agent", "SuperManGa/0.1");
    let key = api_key.trim();
    if !key.is_empty() {
        let lower = base.to_ascii_lowercase();
        if lower.contains("xiaomimimo.com") || lower.contains("mimo.mi.com") {
            req = req.set("api-key", key);
        } else {
            req = req.set("Authorization", &format!("Bearer {key}"));
        }
    }
    let (status, text) = ureq_status_body(req.call())?;
    if !(200..300).contains(&status) {
        return Err(tr_http("list_models", status, &text));
    }
    let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let models = parsed
        .get("data")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("id").and_then(|id| id.as_str()).map(|s| s.to_string()))
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(models)
}

fn http_get(url: &str) -> Result<(u16, String), String> {
    ureq_status_body(
        ureq::get(url)
            .set("User-Agent", "SuperManGa/0.1")
            .call(),
    )
}

fn translate_microsoft(text: &str, target: &str, source: Option<&str>) -> Result<String, String> {
    let from = source
        .filter(|s| !s.is_empty() && *s != "auto")
        .map(|s| map_lang(s, "microsoft"))
        .unwrap_or_default();
    let to = map_lang(target, "microsoft");
    let url = format!(
        "https://edge.microsoft.com/translate/translatetext?from={}&to={}&isEnterpriseClient=false",
        urlencoding_minimal(&from),
        urlencoding_minimal(&to)
    );
    let body = serde_json::to_string(&vec![text]).map_err(|e| e.to_string())?;
    let (status, raw) = http_post_json(&url, &body, &[("Content-Type", "application/json")])?;
    if !(200..300).contains(&status) {
        return Err(tr_http("microsoft", status, &raw));
    }
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    parsed
        .get(0)
        .and_then(|v| v.get("translations"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| tr_empty("microsoft"))
}

fn translate_google(text: &str, target: &str, source: Option<&str>) -> Result<String, String> {
    let sl = source
        .filter(|s| !s.is_empty() && *s != "auto")
        .map(|s| map_lang(s, "google"))
        .unwrap_or_else(|| "auto".into());
    let tl = map_lang(target, "google");
    let url = format!(
        "https://translate.googleapis.com/translate_a/single?client=gtx&dt=t&sl={}&tl={}&q={}",
        urlencoding_minimal(&sl),
        urlencoding_minimal(&tl),
        urlencoding_minimal(text)
    );
    let (status, raw) = http_get(&url)?;
    if !(200..300).contains(&status) {
        return Err(tr_http("google", status, &raw));
    }
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let mut out = String::new();
    if let Some(arr) = parsed.get(0).and_then(|v| v.as_array()) {
        for part in arr {
            if let Some(s) = part.get(0).and_then(|v| v.as_str()) {
                out.push_str(s);
            }
        }
    }
    if out.is_empty() {
        return Err(tr_empty("google"));
    }
    Ok(out)
}

fn translate_yandex(text: &str, target: &str, source: Option<&str>) -> Result<String, String> {
    let lang = if let Some(src) = source.filter(|s| !s.is_empty() && *s != "auto") {
        format!(
            "{}-{}",
            map_lang(src, "yandex"),
            map_lang(target, "yandex")
        )
    } else {
        map_lang(target, "yandex")
    };
    let url = format!(
        "https://translate.yandex.net/api/v1/tr.json/translate?srv=android&lang={}&text={}",
        urlencoding_minimal(&lang),
        urlencoding_minimal(text)
    );
    let (status, raw) = http_get(&url)?;
    if !(200..300).contains(&status) {
        return Err(tr_http("yandex", status, &raw));
    }
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    parsed
        .get("text")
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| tr_empty("yandex"))
}

fn md5_hex(input: &str) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// Baidu Fanyi open platform: https://fanyi-api.baidu.com/api/trans/vip/translate
fn translate_baidu(
    text: &str,
    target: &str,
    settings: &TranslationSettings,
    source: Option<&str>,
) -> Result<String, String> {
    let app_id = settings.baidu_app_id.trim();
    let secret = settings.baidu_secret.trim();
    if app_id.is_empty() || secret.is_empty() {
        return Err(tr_need("TRANSLATE_NEED_BAIDU"));
    }
    let from = source
        .filter(|s| !s.is_empty() && *s != "auto")
        .map(|s| map_lang(s, "baidu"))
        .unwrap_or_else(|| "auto".into());
    let to = map_lang(target, "baidu");
    let salt = format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let sign = md5_hex(&format!("{app_id}{text}{salt}{secret}"));
    let form = format!(
        "q={}&from={}&to={}&appid={}&salt={}&sign={}",
        urlencoding_minimal(text),
        urlencoding_minimal(&from),
        urlencoding_minimal(&to),
        urlencoding_minimal(app_id),
        urlencoding_minimal(&salt),
        urlencoding_minimal(&sign),
    );
    let (status, raw) = http_post_json(
        "https://fanyi-api.baidu.com/api/trans/vip/translate",
        &form,
        &[("Content-Type", "application/x-www-form-urlencoded")],
    )?;
    if !(200..300).contains(&status) {
        return Err(tr_http(
            "baidu",
            status,
            &raw.chars().take(200).collect::<String>(),
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if let Some(err) = parsed.get("error_code") {
        let code = err
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| err.as_i64().map(|n| n.to_string()))
            .unwrap_or_else(|| "?".into());
        let msg = parsed
            .get("error_msg")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Err(format!("TRANSLATE_BAIDU_API:{code}|{msg}"));
    }
    let parts: Vec<String> = parsed
        .get("trans_result")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("dst").and_then(|d| d.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if parts.is_empty() {
        return Err(tr_empty("baidu"));
    }
    Ok(parts.join("\n"))
}

fn translate_deepl(
    text: &str,
    target: &str,
    settings: &TranslationSettings,
    source: Option<&str>,
) -> Result<String, String> {
    let api_key = settings.deepl_api_key.trim();
    if api_key.is_empty() {
        return Err(tr_need("TRANSLATE_NEED_DEEPL"));
    }
    // Free keys end with ":fx"; otherwise use the Pro endpoint.
    let base = if api_key.ends_with(":fx") {
        "https://api-free.deepl.com/v2"
    } else {
        "https://api.deepl.com/v2"
    };
    let url = format!("{base}/translate");
    let mut form = format!(
        "text={}&target_lang={}",
        urlencoding_minimal(text),
        urlencoding_minimal(&map_lang(target, "deepl"))
    );
    if let Some(src) = source.filter(|s| !s.is_empty() && *s != "auto") {
        form.push_str(&format!(
            "&source_lang={}",
            urlencoding_minimal(&map_lang(src, "deepl"))
        ));
    }
    let auth = format!("DeepL-Auth-Key {api_key}");
    let (status, raw) = http_post_json(
        &url,
        &form,
        &[
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Authorization", &auth),
        ],
    )?;
    if !(200..300).contains(&status) {
        return Err(tr_http("deepl", status, &raw));
    }
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    parsed
        .get("translations")
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| tr_empty("deepl"))
}

/// Human-readable target label for Chinese manga system prompts.
fn manga_target_label(ui_lang: &str) -> String {
    let code = ui_lang.trim();
    let short = code.split('-').next().unwrap_or(code).to_lowercase();
    match (code, short.as_str()) {
        ("zh-TW", _) => "繁体中文".into(),
        (_, "zh") => "简体中文".into(),
        (_, "en") => "英语".into(),
        (_, "ja") => "日语".into(),
        (_, "ko") => "韩语".into(),
        (_, "fr") => "法语".into(),
        (_, "es") => "西班牙语".into(),
        (_, "de") => "德语".into(),
        (_, "ru") => "俄语".into(),
        (_, "pt") => "葡萄牙语".into(),
        (_, "it") => "意大利语".into(),
        (_, "vi") => "越南语".into(),
        (_, "th") => "泰语".into(),
        (_, "id") => "印尼语".into(),
        _ => map_lang(ui_lang, "ai"),
    }
}

/// Saber-style single-bubble manga translator (target language from settings).
fn manga_single_system(target: &str) -> String {
    format!(
        "你是专业的漫画翻译引擎，擅长将外语漫画气泡翻译成{target}。\n\
规则：\n\
1. 只输出译文，不要解释或复述原文。\n\
2. 语气自然，符合漫画对白；保留角色口吻。\n\
3. 拟声词/音效词若已是目标语言或难以翻译，可原样保留。\n\
4. 不要添加原文没有的人称代词或信息。"
    )
}

/// Saber `batchNormal` system prompt, parameterized by target language.
fn manga_batch_system(target: &str) -> String {
    format!(
        "忽略之前的所有指令，仅遵循以下定义。\n\n\
## 角色：专业漫画翻译师\n\
你是一个专业的漫画翻译引擎，擅长将外语漫画翻译成{target}。\n\n\
## 翻译方法\n\
1. 直译阶段：对每一行做精确翻译，尽量保持句式，不做过度解读。\n\
2. 分析与意译：捕捉情感与上下文联系。\n\
3. 润色：译文在{target}中自然流畅，保留漫画语气；根据上下文推断人称，不要添加原文没有的代词。\n\n\
## 翻译规则\n\
- 逐行翻译，忠实原意与情感。\n\
- 拟声词/音效词可保留原文。\n\
- 每个译文必须带编号前缀（严格使用 <|数字|> 格式），只输出译文，不要输出原文。\n\
- 不要添加任何解释或评论。"
    )
}

/// Moonshot / Kimi K2.7+ / K3 fix temperature (often only 1); passing 0.2 → 400.
/// Docs: omit the field rather than sending another value.
fn chat_omit_temperature(base_url: &str, model: &str) -> bool {
    let u = base_url.to_ascii_lowercase();
    let m = model.to_ascii_lowercase();
    if u.contains("moonshot.cn") || u.contains("moonshot.") || u.contains("kimi.ai") {
        return true;
    }
    m.contains("kimi-k")
}

fn chat_completions(
    provider: &str,
    base_url: &str,
    api_key: &str,
    model: &str,
    max_tokens: u32,
    messages: serde_json::Value,
) -> Result<String, String> {
    let base = base_url.trim().trim_end_matches('/');
    let model = model.trim();
    if base.is_empty() || model.is_empty() {
        return Err(tr_need("TRANSLATE_NEED_AI"));
    }
    let url = format!("{base}/chat/completions");
    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens.max(256),
        "messages": messages,
    });
    if !chat_omit_temperature(base, model) {
        body["temperature"] = serde_json::json!(0.2);
    }
    let mut headers: Vec<(&str, &str)> = vec![("Content-Type", "application/json")];
    let auth;
    let key = api_key.trim();
    if !key.is_empty() {
        auth = format!("Bearer {key}");
        headers.push(("Authorization", auth.as_str()));
    }
    let (status, raw) = http_post_json(&url, &body.to_string(), &headers)?;
    if !(200..300).contains(&status) {
        return Err(tr_http(provider, status, &raw));
    }
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    parsed
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| tr_empty(provider))
}

fn translate_ai(
    text: &str,
    target: &str,
    settings: &TranslationSettings,
    _source: Option<&str>,
) -> Result<String, String> {
    let target_label = manga_target_label(target);
    let system = manga_single_system(&target_label);
    let messages = serde_json::json!([
        { "role": "system", "content": system },
        { "role": "user", "content": text }
    ]);
    chat_completions(
        "ai",
        &settings.ai_base_url,
        &settings.ai_api_key,
        &settings.ai_model,
        settings.ai_max_tokens,
        messages,
    )
}

fn caiyun_trans_type(target: &str) -> Result<&'static str, String> {
    let short = target
        .trim()
        .split('-')
        .next()
        .unwrap_or(target)
        .to_lowercase();
    match short.as_str() {
        "zh" => Ok("auto2zh"),
        "en" => Ok("auto2en"),
        "ja" => Ok("auto2ja"),
        _ => Err(format!("TRANSLATE_CAIYUN_TARGET:{target}")),
    }
}

fn translate_caiyun(
    texts: &[String],
    target: &str,
    settings: &TranslationSettings,
) -> Result<Vec<String>, String> {
    let token = settings.caiyun_token.trim();
    if token.is_empty() {
        return Err(tr_need("TRANSLATE_NEED_CAIYUN_TOKEN"));
    }
    if texts.is_empty() {
        return Ok(vec![]);
    }
    let trans_type = caiyun_trans_type(target)?;
    let auth = format!("token {token}");
    let body = serde_json::json!({
        "source": texts,
        "trans_type": trans_type,
        "request_id": format!("supermanga-{}", uuid_lite()),
        "detect": true,
        "media": "text",
    });
    let (status, raw) = http_post_json(
        "https://api.interpreter.caiyunai.com/v1/translator",
        &body.to_string(),
        &[
            ("Content-Type", "application/json"),
            ("X-Authorization", &auth),
        ],
    )?;
    if !(200..300).contains(&status) {
        return Err(tr_http(
            "caiyun",
            status,
            &raw.chars().take(200).collect::<String>(),
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let target_arr = parsed
        .get("target")
        .and_then(|v| v.as_array())
        .ok_or_else(|| tr_empty("caiyun"))?;
    if target_arr.len() != texts.len() {
        return Err("TRANSLATE_CAIYUN_COUNT".into());
    }
    Ok(target_arr
        .iter()
        .map(|v| v.as_str().unwrap_or("").trim().to_string())
        .collect())
}

fn uuid_lite() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

fn unix_now_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// UTC Y-M-D from Unix seconds (Howard Hinnant civil_from_days).
fn unix_utc_ymd(secs: u64) -> (i32, u32, u32) {
    let z = (secs / 86400) as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d)
}

fn unix_utc_hms(secs: u64) -> (u32, u32, u32) {
    let tod = secs % 86_400;
    (tod as u32 / 3600, (tod as u32 % 3600) / 60, tod as u32 % 60)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC_SHA256");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hmac_sha1_base64(key: &[u8], data: &[u8]) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use hmac::{Hmac, Mac};
    use sha1::Sha1;
    type HmacSha1 = Hmac<Sha1>;
    let mut mac = HmacSha1::new_from_slice(key).expect("HMAC_SHA1");
    mac.update(data);
    STANDARD.encode(mac.finalize().into_bytes())
}

fn urlencoding_minimal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Aliyun OpenAPI percent-encoding (stricter than RFC3986 for form body).
fn percent_encode_aliyun(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn youdao_input(q: &str) -> String {
    let chars: Vec<char> = q.chars().collect();
    if chars.len() <= 20 {
        return q.to_string();
    }
    let head: String = chars[..10].iter().collect();
    let tail: String = chars[chars.len() - 10..].iter().collect();
    format!("{head}{}{tail}", chars.len())
}

fn translate_youdao(
    text: &str,
    target: &str,
    settings: &TranslationSettings,
    source: Option<&str>,
) -> Result<String, String> {
    let app_key = settings.youdao_app_key.trim();
    let app_secret = settings.youdao_app_secret.trim();
    if app_key.is_empty() || app_secret.is_empty() {
        return Err(tr_need("TRANSLATE_NEED_YOUDAO"));
    }
    let salt = uuid_lite();
    let curtime = unix_now_secs().to_string();
    let sign_src = format!(
        "{app_key}{}{salt}{curtime}{app_secret}",
        youdao_input(text)
    );
    let sign = sha256_hex(sign_src.as_bytes());
    let from = source
        .filter(|s| !s.is_empty() && *s != "auto")
        .map(|s| map_lang(s, "youdao"))
        .unwrap_or_else(|| "auto".into());
    let to = map_lang(target, "youdao");
    let form = format!(
        "q={}&from={}&to={}&appKey={}&salt={}&sign={}&signType=v3&curtime={}",
        urlencoding_minimal(text),
        urlencoding_minimal(&from),
        urlencoding_minimal(&to),
        urlencoding_minimal(app_key),
        urlencoding_minimal(&salt),
        urlencoding_minimal(&sign),
        urlencoding_minimal(&curtime),
    );
    let (status, raw) = http_post_json(
        "https://openapi.youdao.com/api",
        &form,
        &[("Content-Type", "application/x-www-form-urlencoded")],
    )?;
    if !(200..300).contains(&status) {
        return Err(tr_http(
            "youdao",
            status,
            &raw.chars().take(200).collect::<String>(),
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let err = parsed
        .get("errorCode")
        .and_then(|v| v.as_str())
        .unwrap_or("0");
    if err != "0" {
        return Err(format!("TRANSLATE_YOUDAO_API:{err}"));
    }
    let out = parsed
        .get("translation")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
        .trim()
        .to_string();
    if out.is_empty() {
        return Err(tr_empty("youdao"));
    }
    Ok(out)
}

fn translate_tencent(
    text: &str,
    target: &str,
    settings: &TranslationSettings,
    source: Option<&str>,
) -> Result<String, String> {
    let secret_id = settings.tencent_secret_id.trim();
    let secret_key = settings.tencent_secret_key.trim();
    if secret_id.is_empty() || secret_key.is_empty() {
        return Err(tr_need("TRANSLATE_NEED_TENCENT"));
    }
    let region = {
        let r = settings.tencent_region.trim();
        if r.is_empty() {
            "ap-guangzhou"
        } else {
            r
        }
    };
    let host = "tmt.tencentcloudapi.com";
    let service = "tmt";
    let action = "TextTranslate";
    let version = "2018-03-21";
    let timestamp = unix_now_secs();
    let (y, m, d) = unix_utc_ymd(timestamp);
    let date = format!("{y:04}-{m:02}-{d:02}");
    let from = source
        .filter(|s| !s.is_empty() && *s != "auto")
        .map(|s| map_lang(s, "tencent"))
        .unwrap_or_else(|| "auto".into());
    let payload = serde_json::json!({
        "SourceText": text,
        "Source": from,
        "Target": map_lang(target, "tencent"),
        "ProjectId": 0,
    })
    .to_string();
    let hashed_payload = sha256_hex(payload.as_bytes());
    let canonical_headers = format!(
        "content-type:application/json; charset=utf-8\nhost:{host}\nx-tc-action:{}\n",
        action.to_ascii_lowercase()
    );
    let signed_headers = "content-type;host;x-tc-action";
    let canonical_request = format!(
        "POST\n/\n\n{canonical_headers}\n{signed_headers}\n{hashed_payload}"
    );
    let credential_scope = format!("{date}/{service}/tc3_request");
    let string_to_sign = format!(
        "TC3-HMAC-SHA256\n{timestamp}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let secret_date = hmac_sha256(format!("TC3{secret_key}").as_bytes(), date.as_bytes());
    let secret_service = hmac_sha256(&secret_date, service.as_bytes());
    let secret_signing = hmac_sha256(&secret_service, b"tc3_request");
    let signature = hex::encode(hmac_sha256(&secret_signing, string_to_sign.as_bytes()));
    let authorization = format!(
        "TC3-HMAC-SHA256 Credential={secret_id}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );
    let ts = timestamp.to_string();
    let (status, raw) = http_post_json(
        &format!("https://{host}"),
        &payload,
        &[
            ("Authorization", &authorization),
            ("Content-Type", "application/json; charset=utf-8"),
            ("Host", host),
            ("X-TC-Action", action),
            ("X-TC-Timestamp", &ts),
            ("X-TC-Version", version),
            ("X-TC-Region", region),
        ],
    )?;
    if !(200..300).contains(&status) {
        return Err(tr_http(
            "tencent",
            status,
            &raw.chars().take(200).collect::<String>(),
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if let Some(msg) = parsed
        .pointer("/Response/Error/Message")
        .and_then(|v| v.as_str())
    {
        let lower = msg.to_ascii_lowercase();
        if lower.contains("service has not been opened")
            || lower.contains("not been activated")
            || lower.contains("service is not activated")
        {
            return Err("TRANSLATE_TENCENT_NOT_OPENED".into());
        }
        return Err(format!("TRANSLATE_TENCENT_API:{msg}"));
    }
    let out = parsed
        .pointer("/Response/TargetText")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if out.is_empty() {
        return Err(tr_empty("tencent"));
    }
    Ok(out)
}

/// Volc Secret Access Key is Base64; restore missing `=` / `==` padding.
fn restore_base64_padding(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty()
        || !t
            .bytes()
            .all(|b| matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/'))
    {
        return t.to_string();
    }
    match t.len() % 4 {
        2 => format!("{t}=="),
        3 => format!("{t}="),
        _ => t.to_string(),
    }
}

fn translate_volcengine(
    text: &str,
    target: &str,
    settings: &TranslationSettings,
    source: Option<&str>,
) -> Result<String, String> {
    let access_key_id = settings.volc_access_key_id.trim();
    let secret_key = restore_base64_padding(&settings.volc_secret_key);
    if access_key_id.is_empty() || secret_key.is_empty() {
        return Err(tr_need("TRANSLATE_NEED_VOLC"));
    }
    let region = {
        let r = settings.volc_region.trim();
        if r.is_empty() {
            "cn-north-1"
        } else {
            r
        }
    };
    let host = "translate.volcengineapi.com";
    let service = "translate";
    let method = "POST";
    let path = "/";
    let query = "Action=TranslateText&Version=2020-06-01";
    let now = unix_now_secs();
    let (y, m, d) = unix_utc_ymd(now);
    let (hh, mm, ss) = unix_utc_hms(now);
    let x_date = format!("{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}Z");
    let short_date = format!("{y:04}{m:02}{d:02}");
    let mut payload_obj = serde_json::json!({
        "TargetLanguage": map_lang(target, "volcengine"),
        "TextList": [text],
    });
    if let Some(src) = source.filter(|s| !s.is_empty() && *s != "auto") {
        payload_obj["SourceLanguage"] = serde_json::Value::String(map_lang(src, "volcengine"));
    }
    let payload = payload_obj.to_string();
    let x_content_sha256 = sha256_hex(payload.as_bytes());
    let canonical_headers = format!(
        "content-type:application/json\nhost:{host}\nx-content-sha256:{x_content_sha256}\nx-date:{x_date}"
    );
    let signed_headers = "content-type;host;x-content-sha256;x-date";
    let canonical_request = format!(
        "{method}\n{path}\n{query}\n{canonical_headers}\n\n{signed_headers}\n{x_content_sha256}"
    );
    let credential_scope = format!("{short_date}/{region}/{service}/request");
    let string_to_sign = format!(
        "HMAC-SHA256\n{x_date}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let k_date = hmac_sha256(secret_key.as_bytes(), short_date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"request");
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));
    let authorization = format!(
        "HMAC-SHA256 Credential={access_key_id}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );
    let (status, raw) = http_post_json(
        &format!("https://{host}/?{query}"),
        &payload,
        &[
            ("Authorization", &authorization),
            ("Content-Type", "application/json"),
            ("Host", host),
            ("X-Date", &x_date),
            ("X-Content-Sha256", &x_content_sha256),
        ],
    )?;
    if !(200..300).contains(&status) {
        return Err(tr_http(
            "volcengine",
            status,
            &raw.chars().take(200).collect::<String>(),
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if let Some(msg) = parsed
        .pointer("/ResponseMetadata/Error/Message")
        .and_then(|v| v.as_str())
    {
        return Err(format!("TRANSLATE_VOLC_API:{msg}"));
    }
    let out = parsed
        .pointer("/TranslationList/0/Translation")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if out.is_empty() {
        return Err(tr_empty("volcengine"));
    }
    Ok(out)
}

fn translate_aliyun(
    text: &str,
    target: &str,
    settings: &TranslationSettings,
    source: Option<&str>,
) -> Result<String, String> {
    let access_key_id = settings.aliyun_access_key_id.trim();
    let access_key_secret = settings.aliyun_access_key_secret.trim();
    if access_key_id.is_empty() || access_key_secret.is_empty() {
        return Err(tr_need("TRANSLATE_NEED_ALIYUN"));
    }
    let now = unix_now_secs();
    let (y, m, d) = unix_utc_ymd(now);
    let (hh, mm, ss) = unix_utc_hms(now);
    let timestamp = format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z");
    let from = source
        .filter(|s| !s.is_empty() && *s != "auto")
        .map(|s| map_lang(s, "aliyun"))
        .unwrap_or_else(|| "auto".into());
    let mut params: Vec<(String, String)> = vec![
        ("Format".into(), "JSON".into()),
        ("Version".into(), "2018-10-12".into()),
        ("AccessKeyId".into(), access_key_id.to_string()),
        ("SignatureMethod".into(), "HMAC-SHA1".into()),
        ("Timestamp".into(), timestamp),
        ("SignatureVersion".into(), "1.0".into()),
        ("SignatureNonce".into(), uuid_lite()),
        ("Action".into(), "TranslateGeneral".into()),
        ("FormatType".into(), "text".into()),
        ("SourceLanguage".into(), from),
        ("TargetLanguage".into(), map_lang(target, "aliyun")),
        ("SourceText".into(), text.to_string()),
        ("Scene".into(), "general".into()),
    ];
    params.sort_by(|a, b| a.0.cmp(&b.0));
    let canonical = params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode_aliyun(k), percent_encode_aliyun(v)))
        .collect::<Vec<_>>()
        .join("&");
    let string_to_sign = format!(
        "POST&{}&{}",
        percent_encode_aliyun("/"),
        percent_encode_aliyun(&canonical)
    );
    let signature = hmac_sha1_base64(
        format!("{access_key_secret}&").as_bytes(),
        string_to_sign.as_bytes(),
    );
    params.push(("Signature".into(), signature));
    let body = params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode_aliyun(k), percent_encode_aliyun(v)))
        .collect::<Vec<_>>()
        .join("&");
    let (status, raw) = http_post_json(
        "https://mt.cn-hangzhou.aliyuncs.com/",
        &body,
        &[("Content-Type", "application/x-www-form-urlencoded")],
    )?;
    if !(200..300).contains(&status) {
        return Err(tr_http(
            "aliyun",
            status,
            &raw.chars().take(200).collect::<String>(),
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let code = parsed
        .get("Code")
        .map(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .or_else(|| v.as_i64().map(|n| n.to_string()))
                .unwrap_or_else(|| "?".into())
        })
        .unwrap_or_else(|| "200".into());
    if code != "200" {
        let msg = parsed
            .get("Message")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        return Err(format!("TRANSLATE_ALIYUN_API:{code}|{msg}"));
    }
    let out = parsed
        .pointer("/Data/Translated")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if out.is_empty() {
        return Err(tr_empty("aliyun"));
    }
    Ok(out)
}

fn translate_once(
    provider: &str,
    text: &str,
    target: &str,
    settings: &TranslationSettings,
    source: Option<&str>,
) -> Result<String, String> {
    match provider {
        "microsoft" => translate_microsoft(text, target, source),
        "google" => translate_google(text, target, source),
        "yandex" => translate_yandex(text, target, source),
        "deepl" => translate_deepl(text, target, settings, source),
        "baidu" => translate_baidu(text, target, settings, source),
        "youdao" => translate_youdao(text, target, settings, source),
        "tencent" => translate_tencent(text, target, settings, source),
        "volcengine" => translate_volcengine(text, target, settings, source),
        "aliyun" => translate_aliyun(text, target, settings, source),
        "ai" => translate_ai(text, target, settings, source),
        "caiyun" => {
            let out = translate_caiyun(&[text.to_string()], target, settings)?;
            out.into_iter()
                .next()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| tr_empty("caiyun"))
        }
        other => Err(format!("TRANSLATE_PROVIDER:{other}")),
    }
}

/// Translate a page of numbered OCR lines for manga context.
pub fn translate_regions_batch(
    lines: &[(String, String)],
    settings: &TranslationSettings,
    source_lang: Option<&str>,
    cancel: Option<&dyn Fn() -> Result<(), String>>,
) -> Result<Vec<String>, String> {
    if lines.is_empty() {
        return Ok(vec![]);
    }
    if let Some(check) = cancel {
        check()?;
    }
    let provider = settings.provider.as_str();
    let target = settings.target_lang.as_str();
    let source = source_lang
        .map(str::to_string)
        .or_else(|| Some(settings.source_lang.clone()));
    let source_ref = source.as_deref().filter(|s| !s.is_empty());

    // Saber-style AI batch: <|n|> lines + manga system prompt + few-shot.
    if provider == "ai" {
        if let Some(check) = cancel {
            check()?;
        }
        let translated = translate_ai_batch(lines, target, settings)?;
        return Ok(parse_numbered_translations(&translated, lines));
    }

    if provider == "caiyun" {
        let mut texts: Vec<String> = Vec::new();
        let mut index_map: Vec<usize> = Vec::new();
        for (i, (_id, text)) in lines.iter().enumerate() {
            if text.trim().is_empty() {
                continue;
            }
            texts.push(text.replace('\n', " ").replace('\r', ""));
            index_map.push(i);
        }
        if texts.is_empty() {
            return Ok(vec![String::new(); lines.len()]);
        }
        if let Some(check) = cancel {
            check()?;
        }
        let parts = translate_caiyun(&texts, target, settings)?;
        let mut out = vec![String::new(); lines.len()];
        for (part, &idx) in parts.iter().zip(index_map.iter()) {
            out[idx] = part.clone();
        }
        return Ok(out);
    }

    // Baidu: one request with newline-separated segments (lower QPS pressure).
    if provider == "baidu" {
        let mut packed = String::new();
        let mut index_map: Vec<usize> = Vec::new();
        for (i, (_id, text)) in lines.iter().enumerate() {
            if text.trim().is_empty() {
                continue;
            }
            if !packed.is_empty() {
                packed.push('\n');
            }
            // Keep each region as one segment; flatten inner newlines to spaces.
            packed.push_str(&text.replace('\n', " ").replace('\r', ""));
            index_map.push(i);
        }
        if packed.is_empty() {
            return Ok(vec![String::new(); lines.len()]);
        }
        if let Some(check) = cancel {
            check()?;
        }
        let translated = translate_baidu(&packed, target, settings, source_ref)?;
        let parts: Vec<&str> = translated.lines().collect();
        let mut out = vec![String::new(); lines.len()];
        if parts.len() == index_map.len() {
            for (part, &idx) in parts.iter().zip(index_map.iter()) {
                out[idx] = (*part).to_string();
            }
            return Ok(out);
        }
        // Fallback: per-region requests if line count mismatched.
    }

    let mut out = Vec::with_capacity(lines.len());
    for (_id, text) in lines {
        if let Some(check) = cancel {
            check()?;
        }
        if text.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        out.push(translate_once(provider, text, target, settings, source_ref)?);
    }
    Ok(out)
}

const MANGA_FEW_SHOT_SRC: &str = concat!(
    "<|1|>恥ずかしい… 目立ちたくない… 私が消えたい…\n",
    "<|2|>きみ… 大丈夫⁉\n",
    "<|3|>なんだこいつ 空気読めて ないのか…？\n",
);

/// JP→target few-shot for batch manga prompts. Same JP source; translations switch by target.
fn manga_batch_few_shot(target: &str) -> Option<String> {
    let code = target.trim();
    let short = code.split('-').next().unwrap_or(code).to_lowercase();
    let dst: &str = match (code, short.as_str()) {
        ("zh-TW", _) => concat!(
            "<|1|>好尷尬…我不想引人注目…我想消失…\n",
            "<|2|>你…沒事吧⁉\n",
            "<|3|>這傢伙怎麼看不懂氣氛的…？\n",
        ),
        (_, "zh") => concat!(
            "<|1|>好尴尬…我不想引人注目…我想消失…\n",
            "<|2|>你…没事吧⁉\n",
            "<|3|>这家伙怎么看不懂气氛的…？\n",
        ),
        (_, "en") => concat!(
            "<|1|>So embarrassing… I don't want to stand out… I wish I could disappear…\n",
            "<|2|>Hey… are you okay⁉\n",
            "<|3|>What's with this guy? Can't he read the room…?\n",
        ),
        (_, "ko") => concat!(
            "<|1|>창피해… 눈에 띄기 싫어… 사라지고 싶어…\n",
            "<|2|>너… 괜찮아⁉\n",
            "<|3|>이 녀석 분위기 파악을 못 하는 건가…?\n",
        ),
        (_, "ja") => MANGA_FEW_SHOT_SRC,
        (_, "fr") => concat!(
            "<|1|>C'est trop gênant… Je ne veux pas me faire remarquer… Je voudrais disparaître…\n",
            "<|2|>Toi… ça va⁉\n",
            "<|3|>C'est quoi ce type ? Il capte pas l'ambiance… ?\n",
        ),
        (_, "es") => concat!(
            "<|1|>Qué vergüenza… No quiero llamar la atención… Quiero desaparecer…\n",
            "<|2|>Oye… ¿estás bien⁉\n",
            "<|3|>¿Qué le pasa a este? ¿No lee el ambiente…?\n",
        ),
        (_, "de") => concat!(
            "<|1|>So peinlich… Ich will nicht auffallen… Ich will verschwinden…\n",
            "<|2|>Hey… alles okay⁉\n",
            "<|3|>Was ist mit dem Typen? Kann er die Stimmung nicht lesen…?\n",
        ),
        (_, "ru") => concat!(
            "<|1|>Как стыдно… Не хочу привлекать внимание… Хочу исчезнуть…\n",
            "<|2|>Эй… ты в порядке⁉\n",
            "<|3|>Что с этим парнем? Не чувствует атмосферу…?\n",
        ),
        (_, "vi") => concat!(
            "<|1|>Xấu hổ quá… Không muốn nổi bật… Muốn biến mất…\n",
            "<|2|>Này… cậu ổn chứ⁉\n",
            "<|3|>Thằng này sao thế? Không đọc được không khí à…?\n",
        ),
        _ => return None,
    };
    Some(format!("{MANGA_FEW_SHOT_SRC}---\n{dst}"))
}

fn translate_ai_batch(
    lines: &[(String, String)],
    target: &str,
    settings: &TranslationSettings,
) -> Result<String, String> {
    let target_label = manga_target_label(target);
    let system = manga_batch_system(&target_label);
    let mut user = format!(
        "请帮我将以下漫画文本翻译成{target_label}。如果文本已经是目标语言或看起来是拟声词/音效词，请原样输出。保持 <|数字|> 编号前缀。\n\n"
    );
    // Prefer a target-matched few-shot; skip when we have no curated example
    // so a Chinese demo does not pull EN/KO (etc.) output toward 简中.
    if let Some(shot) = manga_batch_few_shot(target) {
        user.push_str(&shot);
        user.push_str("---\n");
    }
    for (i, (_id, text)) in lines.iter().enumerate() {
        let n = i + 1;
        let flat = text.replace('\n', " ").replace('\r', "");
        user.push_str(&format!("<|{n}|>{flat}\n"));
    }
    let messages = serde_json::json!([
        { "role": "system", "content": system },
        { "role": "user", "content": user }
    ]);
    chat_completions(
        "ai",
        &settings.ai_base_url,
        &settings.ai_api_key,
        &settings.ai_model,
        settings.ai_max_tokens.max(1024),
        messages,
    )
}

fn parse_numbered_translations(raw: &str, lines: &[(String, String)]) -> Vec<String> {
    let mut map = std::collections::HashMap::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Saber: <|1|>译文
        if let Some(rest) = trimmed.strip_prefix("<|") {
            if let Some((num, after)) = rest.split_once("|>") {
                let key = num.trim().to_string();
                if !key.is_empty() {
                    map.insert(key, after.trim().to_string());
                    continue;
                }
            }
        }
        // Fallback: "1. text" / "r0: text"
        if let Some((num, rest)) = trimmed.split_once(['.', '、', ':', '：']) {
            let key = num.trim().trim_start_matches('r').to_string();
            if key.chars().all(|c| c.is_ascii_digit()) {
                map.insert(key, rest.trim().to_string());
            }
        }
    }
    lines
        .iter()
        .enumerate()
        .map(|(i, (id, text))| {
            let by_idx = map.get(&(i + 1).to_string()).cloned();
            let by_id = map.get(id).cloned();
            by_idx
                .or(by_id)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| text.clone())
        })
        .collect()
}

pub fn translate_text(req: TranslationRequest) -> Result<TranslationResponse, String> {
    let text = req.text;
    if text.trim().is_empty() {
        return Ok(TranslationResponse::err(tr_need("TRANSLATE_EMPTY_INPUT")));
    }
    let provider = req.settings.provider.clone();
    let target = req.settings.target_lang.clone();
    let source = req
        .source_lang
        .as_deref()
        .or(Some(req.settings.source_lang.as_str()));
    match translate_once(
        &provider,
        &text,
        &target,
        &req.settings,
        source.filter(|s| !s.is_empty()),
    ) {
        Ok(translated) => Ok(TranslationResponse::ok(translated, provider)),
        Err(message) => Ok(TranslationResponse::err(message)),
    }
}
