
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::chat_manager::request as chat_request;
use crate::chat_manager::request_builder::effective_streaming_enabled;
use crate::chat_manager::types::ProviderCredential;
use crate::creation_helper::service::send_creation_api_request;
use crate::storage_manager::settings::internal_read_settings;

pub struct LlmContext {
    pub cred: ProviderCredential,
    pub api_key: String,
    pub provider_id: String,
    pub provider_label: String,
    pub model_id: String,
    pub model_name: String,
    pub streaming_enabled: bool,
}

pub fn load_context(app: &AppHandle) -> Result<LlmContext, String> {
    let settings_json =
        internal_read_settings(app)?.ok_or_else(|| "No settings found".to_string())?;
    let settings: Value =
        serde_json::from_str(&settings_json).map_err(|e| format!("settings parse: {}", e))?;
    let advanced = settings.get("advancedSettings");

    let model_id = advanced
        .and_then(|a| a.get("creationHelperModelId"))
        .and_then(|v| v.as_str())
        .or_else(|| settings.get("defaultModelId").and_then(|v| v.as_str()))
        .ok_or_else(|| "No model configured".to_string())?
        .to_string();

    let streaming_enabled_setting = advanced
        .and_then(|a| a.get("creationHelperStreaming"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let models = settings.get("models").and_then(|v| v.as_array());
    let model = models
        .and_then(|m| {
            m.iter()
                .find(|m| m.get("id").and_then(|v| v.as_str()) == Some(model_id.as_str()))
        })
        .ok_or_else(|| "Model not found".to_string())?;

    let provider_id = model
        .get("providerId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let model_name = model
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let credentials = settings.get("providerCredentials").and_then(|v| v.as_array());
    let credential = credentials
        .and_then(|c| {
            c.iter().find(|cred| {
                cred.get("providerId").and_then(|v| v.as_str()) == Some(provider_id.as_str())
            })
        })
        .ok_or_else(|| "No credentials found for provider".to_string())?;

    let api_key = credential
        .get("apiKey")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let base_url = credential
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let provider_label = credential
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or(provider_id.as_str())
        .to_string();

    let cred = ProviderCredential {
        id: credential
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        provider_id: provider_id.clone(),
        label: provider_label.clone(),
        api_key: Some(api_key.clone()),
        base_url,
        default_model: None,
        headers: credential
            .get("headers")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok()),
        config: credential.get("config").cloned(),
    };

    let streaming_enabled = effective_streaming_enabled(&cred, streaming_enabled_setting);

    Ok(LlmContext {
        cred,
        api_key,
        provider_id,
        provider_label,
        model_id,
        model_name,
        streaming_enabled,
    })
}

pub async fn call_text(
    app: &AppHandle,
    ctx: &LlmContext,
    session_id: &str,
    stream_request_id: &str,
    system: &str,
    user: &str,
    visible: bool,
) -> Result<String, String> {
    let messages = vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": user }),
    ];

    let streaming = visible && ctx.streaming_enabled;
    let request_id = if visible {
        stream_request_id.to_string()
    } else {
        format!("{}-internal", stream_request_id)
    };

    let resp = send_creation_api_request(
        app,
        session_id,
        &request_id,
        &ctx.provider_id,
        &ctx.cred,
        &ctx.api_key,
        &ctx.model_name,
        &messages,
        streaming,
        None,
    )
    .await?;

    if !resp.ok {
        let err = resp
            .data()
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("LLM call failed")
            .to_string();
        crate::creation_helper::service::record_creation_usage(
            app,
            resp.data(),
            session_id,
            &ctx.model_id,
            &ctx.model_name,
            &ctx.provider_id,
            &ctx.provider_label,
            &ctx.api_key,
            "",
            false,
            Some(err.clone()),
        )
        .await;
        return Err(err);
    }

    let text = chat_request::extract_text(resp.data(), Some(&ctx.provider_id)).unwrap_or_default();
    crate::creation_helper::service::record_creation_usage(
        app,
        resp.data(),
        session_id,
        &ctx.model_id,
        &ctx.model_name,
        &ctx.provider_id,
        &ctx.provider_label,
        &ctx.api_key,
        "",
        true,
        None,
    )
    .await;
    Ok(text)
}

