use tauri::AppHandle;

use super::binary;
use super::binary::{SdEngineVariant, SdQueuedInstall};
use super::registry;
use super::types::{SdBinaryInfo, SdFamily, SdModelEntry, SdModelEntryDto, SdModelFiles, SdStatus};

#[tauri::command]
pub async fn sd_get_status(app: AppHandle) -> Result<SdStatus, String> {
    Ok(SdStatus {
        binary: binary::read_binary_info(&app),
        recommended_variant: binary::detect_recommended_variant(),
        models_dir: registry::models_dir(&app)?.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn sd_list_models(app: AppHandle) -> Result<Vec<SdModelEntryDto>, String> {
    Ok(registry::list_models(&app)
        .await?
        .into_iter()
        .map(SdModelEntryDto::from)
        .collect())
}

#[tauri::command]
pub async fn sd_import_model(
    app: AppHandle,
    name: String,
    family: String,
    files: SdModelFiles,
) -> Result<SdModelEntryDto, String> {
    let family = SdFamily::parse(&family).ok_or_else(|| format!("Unknown family: {family}"))?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Model name is required".to_string());
    }
    if files.all_paths().is_empty() {
        return Err("At least one model file is required".to_string());
    }
    registry::validate_files_exist(&files)?;
    let entry = SdModelEntry {
        id: format!("{}-{}", family.prefix(), uuid::Uuid::new_v4()),
        name,
        family,
        total_bytes: registry::total_bytes(&files),
        files,
        source: "imported".to_string(),
        repo: None,
        created_at: crate::infra::utils::now_millis()?,
    };
    Ok(registry::upsert_model(&app, entry).await?.into())
}

#[tauri::command]
pub async fn sd_update_model_files(
    app: AppHandle,
    model_id: String,
    files: SdModelFiles,
) -> Result<SdModelEntryDto, String> {
    registry::validate_files_exist(&files)?;
    Ok(registry::update_model_files(&app, &model_id, files)
        .await?
        .into())
}

#[tauri::command]
pub async fn sd_list_engine_variants() -> Result<Vec<SdEngineVariant>, String> {
    binary::list_engine_variants().await
}

#[tauri::command]
pub async fn sd_queue_binary_install(
    app: AppHandle,
    variant: Option<String>,
) -> Result<SdQueuedInstall, String> {
    binary::queue_binary_install(&app, variant).await
}

#[tauri::command]
pub async fn sd_finalize_binary_install(app: AppHandle) -> Result<SdBinaryInfo, String> {
    binary::finalize_binary_install(&app)
}

#[tauri::command]
pub async fn sd_remove_binary(app: AppHandle) -> Result<(), String> {
    binary::remove_binary(&app)
}

#[tauri::command]
pub async fn sd_cancel_generation() -> Result<bool, String> {
    super::generate::cancel().await
}

#[tauri::command]
pub async fn sd_register_hf_model(
    app: AppHandle,
    repo: String,
    file_path: String,
    role: String,
    family: String,
    display_name: Option<String>,
) -> Result<SdModelEntryDto, String> {
    let family = SdFamily::parse(&family).ok_or_else(|| format!("Unknown family: {family}"))?;
    if !std::path::PathBuf::from(&file_path).is_file() {
        return Err(format!("File not found: {file_path}"));
    }

    let mut files = SdModelFiles::default();
    match role.as_str() {
        "checkpoint" => files.checkpoint = Some(file_path),
        "diffusionModel" => files.diffusion_model = Some(file_path),
        "clipL" => files.clip_l = Some(file_path),
        "clipG" => files.clip_g = Some(file_path),
        "t5xxl" => files.t5xxl = Some(file_path),
        "vae" => files.vae = Some(file_path),
        other => return Err(format!("Unknown file role: {other}")),
    }

    if let Some(existing) = registry::find_by_repo(&app, &repo).await? {
        if existing.family == family {
            return Ok(registry::update_model_files(&app, &existing.id, files)
                .await?
                .into());
        }
    }

    let name = display_name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            repo.split('/')
                .next_back()
                .unwrap_or(repo.as_str())
                .to_string()
        });
    let entry = SdModelEntry {
        id: format!("{}-{}", family.prefix(), uuid::Uuid::new_v4()),
        name,
        family,
        total_bytes: registry::total_bytes(&files),
        files,
        source: "hf".to_string(),
        repo: Some(repo),
        created_at: crate::infra::utils::now_millis()?,
    };
    Ok(registry::upsert_model(&app, entry).await?.into())
}

#[tauri::command]
pub async fn sd_delete_model(
    app: AppHandle,
    model_id: String,
    delete_files: bool,
) -> Result<bool, String> {
    let removed = registry::remove_model(&app, &model_id).await?;
    let Some(entry) = removed else {
        return Ok(false);
    };
    if delete_files {
        let models_root = registry::models_dir(&app)?;
        for path in entry.files.all_paths() {
            let path = std::path::PathBuf::from(path);
            if path.starts_with(&models_root) && path.is_file() {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    Ok(true)
}
