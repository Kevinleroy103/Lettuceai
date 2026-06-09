use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

use super::{parse_size_dimensions, ImageProviderAdapter, ImageRequestPayload, ImageResponseData};
use crate::image_generator::types::ImageGenerationRequest;

pub struct SdcppAdapter;

#[derive(Deserialize)]
struct SdcppResponse {
    images: Vec<String>,
}

fn request_body(request: &ImageGenerationRequest) -> Value {
    let advanced = request.advanced_model_settings.as_ref();
    let size_override = request
        .size
        .as_deref()
        .or_else(|| advanced.and_then(|settings| settings.sd_size.as_deref()));
    let (width, height) = parse_size_dimensions(size_override, 1024, 1024);

    let mut body = Map::new();
    body.insert("prompt".into(), Value::String(request.prompt.clone()));
    body.insert("width".into(), json!(width));
    body.insert("height".into(), json!(height));
    body.insert("batch_size".into(), json!(request.n.unwrap_or(1)));

    if let Some(steps) = advanced.and_then(|settings| settings.sd_steps) {
        body.insert("steps".into(), json!(steps));
    }
    if let Some(cfg_scale) = advanced.and_then(|settings| settings.sd_cfg_scale) {
        body.insert("cfg_scale".into(), json!(cfg_scale));
    }
    if let Some(sampler) = advanced
        .and_then(|settings| settings.sd_sampler.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        body.insert("sampler_name".into(), Value::String(sampler.to_string()));
    }
    if let Some(seed) = advanced.and_then(|settings| settings.sd_seed) {
        body.insert("seed".into(), json!(seed));
    }
    if let Some(negative_prompt) = advanced
        .and_then(|settings| settings.sd_negative_prompt.as_ref())
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        body.insert(
            "negative_prompt".into(),
            Value::String(negative_prompt.to_string()),
        );
    }

    if let Some(images) = request
        .input_images
        .as_ref()
        .filter(|images| !images.is_empty())
    {
        body.insert(
            "extra_images".into(),
            Value::Array(images.iter().cloned().map(Value::String).collect()),
        );
    }

    Value::Object(body)
}

impl ImageProviderAdapter for SdcppAdapter {
    fn endpoint(&self, base_url: &str, _request: &ImageGenerationRequest) -> String {
        let base = base_url
            .trim_end_matches('/')
            .trim_end_matches("/sdapi/v1");
        format!("{}/sdapi/v1/txt2img", base)
    }

    fn required_auth_headers(&self) -> &'static [&'static str] {
        &[]
    }

    fn headers(
        &self,
        _api_key: &str,
        extra: Option<&HashMap<String, String>>,
    ) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".into(), "application/json".into());
        if let Some(extra) = extra {
            for (key, value) in extra {
                headers.insert(key.clone(), value.clone());
            }
        }
        headers
    }

    fn payload(&self, request: &ImageGenerationRequest) -> Result<ImageRequestPayload, String> {
        Ok(ImageRequestPayload::Json(request_body(request)))
    }

    fn parse_response(&self, response: Value) -> Result<Vec<ImageResponseData>, String> {
        let parsed: SdcppResponse = serde_json::from_value(response).map_err(|error| {
            crate::utils::err_msg(
                module_path!(),
                line!(),
                format!("Failed to parse sd.cpp response: {}", error),
            )
        })?;

        if parsed.images.is_empty() {
            return Err(crate::utils::err_msg(
                module_path!(),
                line!(),
                "sd.cpp returned no images",
            ));
        }

        Ok(parsed
            .images
            .into_iter()
            .map(|image| ImageResponseData {
                url: None,
                b64_json: Some(image),
                text: None,
            })
            .collect())
    }
}
