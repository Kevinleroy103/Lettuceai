use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;

use crate::utils::{log_info, log_warn};

pub const PROVIDER_ID: &str = "sdcpp";
pub const PROVIDER_LABEL: &str = "Stable Diffusion (Local)";

const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/leejet/stable-diffusion.cpp/releases/latest";
const DEFAULT_PORT: u16 = 17861;
const READY_TIMEOUT_SECS: u64 = 300;
const STATUS_EVENT: &str = "sdcpp_status";
const DOWNLOAD_EVENT: &str = "sdcpp_download_progress";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SdcppConfig {
    pub backend: String,
    pub port: Option<u16>,
    pub full_model_path: Option<String>,
    pub diffusion_model_path: Option<String>,
    pub vae_path: Option<String>,
    pub llm_path: Option<String>,
    pub clip_l_path: Option<String>,
    pub clip_g_path: Option<String>,
    pub t5xxl_path: Option<String>,
    pub offload_to_cpu: bool,
    pub flash_attention: bool,
    pub keep_alive_minutes: Option<u32>,
    pub extra_args: Option<String>,
}

impl SdcppConfig {
    fn has_model(&self) -> bool {
        self.full_model_path
            .as_deref()
            .map(|p| !p.trim().is_empty())
            .unwrap_or(false)
            || self
                .diffusion_model_path
                .as_deref()
                .map(|p| !p.trim().is_empty())
                .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinaryInstall {
    tag: String,
    backend: String,
    executable: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdcppStatus {
    pub binary_installed: bool,
    pub binary_tag: Option<String>,
    pub binary_backend: Option<String>,
    pub model_configured: bool,
    pub server_running: bool,
    pub server_ready: bool,
    pub base_url: Option<String>,
}

struct ServerProc {
    child: Child,
    port: u16,
    config_fingerprint: String,
    ready: bool,
    last_used: Instant,
}

#[derive(Default)]
pub struct SdcppState {
    inner: Arc<Mutex<Option<ServerProc>>>,
}

fn sdcpp_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = crate::utils::ensure_lettuce_dir(app)?.join("sdcpp");
    fs::create_dir_all(&dir)
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
    Ok(dir)
}

fn bin_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = sdcpp_dir(app)?.join("bin");
    fs::create_dir_all(&dir)
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
    Ok(dir)
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(sdcpp_dir(app)?.join("config.json"))
}

fn install_marker_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(sdcpp_dir(app)?.join("binary.json"))
}

fn read_config(app: &AppHandle) -> Result<SdcppConfig, String> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(SdcppConfig {
            backend: "auto".to_string(),
            ..Default::default()
        });
    }
    let raw = fs::read_to_string(&path)
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
    serde_json::from_str(&raw).map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))
}

fn write_config(app: &AppHandle, config: &SdcppConfig) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(config)
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
    fs::write(config_path(app)?, raw)
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))
}

fn read_install_marker(app: &AppHandle) -> Option<BinaryInstall> {
    let path = install_marker_path(app).ok()?;
    let raw = fs::read_to_string(path).ok()?;
    let install: BinaryInstall = serde_json::from_str(&raw).ok()?;
    if Path::new(&install.executable).exists() {
        Some(install)
    } else {
        None
    }
}

fn config_fingerprint(config: &SdcppConfig) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        config.full_model_path.as_deref().unwrap_or(""),
        config.diffusion_model_path.as_deref().unwrap_or(""),
        config.vae_path.as_deref().unwrap_or(""),
        config.llm_path.as_deref().unwrap_or(""),
        config.clip_l_path.as_deref().unwrap_or(""),
        config.clip_g_path.as_deref().unwrap_or(""),
        config.t5xxl_path.as_deref().unwrap_or(""),
        config.offload_to_cpu,
        config.flash_attention,
        config.extra_args.as_deref().unwrap_or(""),
    )
}

fn emit_status(app: &AppHandle, phase: &str, detail: Option<String>) {
    let _ = app.emit(
        STATUS_EVENT,
        serde_json::json!({ "phase": phase, "detail": detail }),
    );
}

fn nvidia_gpu_present() -> bool {
    Command::new("nvidia-smi")
        .arg("-L")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn resolve_backend(requested: &str) -> String {
    if requested != "auto" && !requested.is_empty() {
        return requested.to_string();
    }
    if cfg!(target_os = "macos") {
        return "metal".to_string();
    }
    if cfg!(target_os = "windows") && nvidia_gpu_present() {
        return "cuda".to_string();
    }
    "vulkan".to_string()
}

fn asset_matches(name: &str, backend: &str) -> bool {
    let lower = name.to_lowercase();
    if !lower.ends_with(".zip") || lower.starts_with("cudart") {
        return false;
    }
    if cfg!(target_os = "macos") {
        return lower.contains("darwin") && lower.contains("arm64");
    }
    if cfg!(target_os = "windows") {
        if !lower.contains("-win-") {
            return false;
        }
        return match backend {
            "cuda" => lower.contains("cuda12"),
            "vulkan" => lower.contains("vulkan"),
            "rocm" => lower.contains("rocm"),
            _ => lower.contains("avx2"),
        };
    }
    if !lower.contains("linux") || !lower.contains("x86_64") {
        return false;
    }
    match backend {
        "vulkan" | "cuda" => lower.contains("vulkan"),
        "rocm" => lower.contains("rocm"),
        _ => {
            !lower.contains("vulkan")
                && !lower.contains("rocm")
                && lower.contains("x86_64")
        }
    }
}

async fn fetch_latest_release(app: &AppHandle) -> Result<Value, String> {
    let client = reqwest::Client::new();
    let response = client
        .get(RELEASES_LATEST_URL)
        .header("User-Agent", "LettuceAI")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
    if !response.status().is_success() {
        return Err(crate::utils::err_msg(
            module_path!(),
            line!(),
            format!("GitHub release lookup failed: {}", response.status()),
        ));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
    log_info(app, "sdcpp", "Fetched latest sd.cpp release metadata");
    Ok(value)
}

async fn download_to_file(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    label: &str,
) -> Result<(), String> {
    use futures_util::StreamExt;

    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("User-Agent", "LettuceAI")
        .send()
        .await
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
    if !response.status().is_success() {
        return Err(crate::utils::err_msg(
            module_path!(),
            line!(),
            format!("Download failed ({}): {}", response.status(), url),
        ));
    }
    let total = response.content_length().unwrap_or(0);
    let tmp = dest.with_extension("tmp");
    let mut file =
        fs::File::create(&tmp).map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emit = Instant::now();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
        file.write_all(&chunk)
            .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed() >= Duration::from_millis(200) {
            last_emit = Instant::now();
            let _ = app.emit(
                DOWNLOAD_EVENT,
                serde_json::json!({
                    "label": label,
                    "downloaded": downloaded,
                    "total": total,
                    "status": "downloading",
                }),
            );
        }
    }
    drop(file);
    fs::rename(&tmp, dest).map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
    let _ = app.emit(
        DOWNLOAD_EVENT,
        serde_json::json!({
            "label": label,
            "downloaded": downloaded,
            "total": total,
            "status": "complete",
        }),
    );
    Ok(())
}

fn unzip_into(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = fs::File::open(archive_path)
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
    archive
        .extract(dest_dir)
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
    Ok(())
}

fn find_server_executable(dir: &Path) -> Option<PathBuf> {
    let server_names: &[&str] = if cfg!(target_os = "windows") {
        &["sd-server.exe", "sd-cpp-server.exe", "server.exe"]
    } else {
        &["sd-server", "sd-cpp-server", "server"]
    };
    let mut stack = vec![dir.to_path_buf()];
    let mut fallback: Option<PathBuf> = None;
    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if server_names.iter().any(|candidate| name == *candidate) {
                return Some(path);
            }
            let is_sd_cli = name == "sd" || name == "sd.exe" || name == "sd-cli" || name == "sd-cli.exe";
            if is_sd_cli && fallback.is_none() {
                fallback = Some(path);
            }
        }
    }
    fallback
}

#[cfg(unix)]
fn make_executable(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(meta) = fs::metadata(&path) {
                let mut perms = meta.permissions();
                perms.set_mode(perms.mode() | 0o755);
                let _ = fs::set_permissions(&path, perms);
            }
        }
    }
}

#[cfg(not(unix))]
fn make_executable(_dir: &Path) {}

#[tauri::command]
pub async fn sdcpp_download_binary(
    app: AppHandle,
    backend: Option<String>,
    state: State<'_, SdcppState>,
) -> Result<String, String> {
    stop_server_internal(&app, &state).await;

    let requested = backend.unwrap_or_else(|| "auto".to_string());
    let resolved_backend = resolve_backend(&requested);
    emit_status(&app, "downloading_binary", Some(resolved_backend.clone()));

    let release = fetch_latest_release(&app).await?;
    let tag = release
        .get("tag_name")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let assets = release
        .get("assets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut chosen: Option<(String, String)> = None;
    let mut cudart: Option<(String, String)> = None;
    for asset in &assets {
        let name = asset.get("name").and_then(Value::as_str).unwrap_or("");
        let url = asset
            .get("browser_download_url")
            .and_then(Value::as_str)
            .unwrap_or("");
        if name.is_empty() || url.is_empty() {
            continue;
        }
        if asset_matches(name, &resolved_backend) && chosen.is_none() {
            chosen = Some((name.to_string(), url.to_string()));
        }
        if name.to_lowercase().starts_with("cudart") && cudart.is_none() {
            cudart = Some((name.to_string(), url.to_string()));
        }
    }

    let (asset_name, asset_url) = chosen.ok_or_else(|| {
        crate::utils::err_msg(
            module_path!(),
            line!(),
            format!(
                "No sd.cpp release asset found for this platform with backend '{}'",
                resolved_backend
            ),
        )
    })?;

    let dir = bin_dir(&app)?;
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;

    let archive_path = dir.join(&asset_name);
    download_to_file(&app, &asset_url, &archive_path, &asset_name).await?;
    unzip_into(&archive_path, &dir)?;
    let _ = fs::remove_file(&archive_path);

    if cfg!(target_os = "windows") && resolved_backend == "cuda" {
        if let Some((cudart_name, cudart_url)) = cudart {
            let cudart_path = dir.join(&cudart_name);
            download_to_file(&app, &cudart_url, &cudart_path, &cudart_name).await?;
            unzip_into(&cudart_path, &dir)?;
            let _ = fs::remove_file(&cudart_path);
        }
    }

    make_executable(&dir);

    let executable = find_server_executable(&dir).ok_or_else(|| {
        crate::utils::err_msg(
            module_path!(),
            line!(),
            "Downloaded archive did not contain an sd-server executable",
        )
    })?;

    let install = BinaryInstall {
        tag: tag.clone(),
        backend: resolved_backend.clone(),
        executable: executable.to_string_lossy().to_string(),
    };
    fs::write(
        install_marker_path(&app)?,
        serde_json::to_string_pretty(&install)
            .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?,
    )
    .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;

    emit_status(&app, "binary_ready", Some(tag.clone()));
    log_info(
        &app,
        "sdcpp",
        format!("Installed sd.cpp {} ({})", tag, resolved_backend),
    );
    Ok(tag)
}

fn build_server_args(config: &SdcppConfig, port: u16) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    if let Some(path) = config
        .full_model_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        args.push("--model".into());
        args.push(path.to_string());
    } else if let Some(path) = config
        .diffusion_model_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        args.push("--diffusion-model".into());
        args.push(path.to_string());
    }
    let optional_flags: [(&str, &Option<String>); 5] = [
        ("--vae", &config.vae_path),
        ("--llm", &config.llm_path),
        ("--clip_l", &config.clip_l_path),
        ("--clip_g", &config.clip_g_path),
        ("--t5xxl", &config.t5xxl_path),
    ];
    for (flag, value) in optional_flags {
        if let Some(path) = value.as_deref().filter(|p| !p.trim().is_empty()) {
            args.push(flag.into());
            args.push(path.to_string());
        }
    }
    if config.offload_to_cpu {
        args.push("--offload-to-cpu".into());
    }
    if config.flash_attention {
        args.push("--diffusion-fa".into());
    }
    args.push("--listen-ip".into());
    args.push("127.0.0.1".into());
    args.push("--listen-port".into());
    args.push(port.to_string());
    if let Some(extra) = config.extra_args.as_deref() {
        for token in extra.split_whitespace() {
            args.push(token.to_string());
        }
    }
    args
}

async fn server_is_ready(base_url: &str) -> bool {
    let client = reqwest::Client::new();
    matches!(
        client
            .get(format!("{}/sdcpp/v1/capabilities", base_url))
            .timeout(Duration::from_secs(3))
            .send()
            .await,
        Ok(response) if response.status().is_success()
    )
}

async fn stop_server_internal(app: &AppHandle, state: &State<'_, SdcppState>) {
    let mut guard = state.inner.lock().await;
    if let Some(mut proc) = guard.take() {
        log_info(app, "sdcpp", "Stopping sd-server");
        let _ = proc.child.kill();
        let _ = proc.child.wait();
        emit_status(app, "stopped", None);
    }
}

pub async fn ensure_server_running(app: &AppHandle) -> Result<String, String> {
    let state = app.state::<SdcppState>();
    let config = read_config(app)?;
    if !config.has_model() {
        return Err(crate::utils::err_msg(
            module_path!(),
            line!(),
            "No local diffusion model configured. Pick a model in Settings → Image Generation.",
        ));
    }
    let install = read_install_marker(app).ok_or_else(|| {
        crate::utils::err_msg(
            module_path!(),
            line!(),
            "sd.cpp runtime is not installed. Download it in Settings → Image Generation.",
        )
    })?;

    let fingerprint = config_fingerprint(&config);
    let port = config.port.unwrap_or(DEFAULT_PORT);
    let base_url = format!("http://127.0.0.1:{}", port);

    {
        let mut guard = state.inner.lock().await;
        if let Some(proc) = guard.as_mut() {
            let alive = proc.child.try_wait().ok().flatten().is_none();
            if alive && proc.ready && proc.config_fingerprint == fingerprint {
                proc.last_used = Instant::now();
                return Ok(format!("http://127.0.0.1:{}", proc.port));
            }
            log_info(
                app,
                "sdcpp",
                "Restarting sd-server (config changed or process died)",
            );
            let _ = proc.child.kill();
            let _ = proc.child.wait();
            *guard = None;
        }
    }

    emit_status(app, "starting", None);
    let args = build_server_args(&config, port);
    log_info(
        app,
        "sdcpp",
        format!("Spawning sd-server: {} {}", install.executable, args.join(" ")),
    );

    let log_path = sdcpp_dir(app)?.join("server.log");
    let log_file = fs::File::create(&log_path)
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;
    let log_file_err = log_file
        .try_clone()
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;

    let mut command = Command::new(&install.executable);
    command
        .args(&args)
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err));
    if let Some(parent) = Path::new(&install.executable).parent() {
        command.current_dir(parent);
    }
    let child = command
        .spawn()
        .map_err(|e| crate::utils::err_to_string(module_path!(), line!(), e))?;

    {
        let mut guard = state.inner.lock().await;
        *guard = Some(ServerProc {
            child,
            port,
            config_fingerprint: fingerprint,
            ready: false,
            last_used: Instant::now(),
        });
    }

    emit_status(app, "loading_model", None);
    let deadline = Instant::now() + Duration::from_secs(READY_TIMEOUT_SECS);
    loop {
        if server_is_ready(&base_url).await {
            break;
        }
        {
            let mut guard = state.inner.lock().await;
            let exited = guard
                .as_mut()
                .and_then(|proc| proc.child.try_wait().ok().flatten());
            if let Some(status) = exited {
                *guard = None;
                let tail = fs::read_to_string(&log_path)
                    .ok()
                    .map(|content| {
                        content
                            .lines()
                            .rev()
                            .take(12)
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                emit_status(app, "error", Some(tail.clone()));
                return Err(crate::utils::err_msg(
                    module_path!(),
                    line!(),
                    format!("sd-server exited early ({}): {}", status, tail),
                ));
            }
        }
        if Instant::now() >= deadline {
            stop_server_internal(app, &state).await;
            return Err(crate::utils::err_msg(
                module_path!(),
                line!(),
                "Timed out waiting for sd-server to load the model",
            ));
        }
        tokio::time::sleep(Duration::from_millis(750)).await;
    }

    {
        let mut guard = state.inner.lock().await;
        if let Some(proc) = guard.as_mut() {
            proc.ready = true;
            proc.last_used = Instant::now();
        }
    }
    emit_status(app, "ready", None);
    log_info(app, "sdcpp", format!("sd-server ready at {}", base_url));

    spawn_idle_reaper(app.clone());

    Ok(base_url)
}

fn spawn_idle_reaper(app: AppHandle) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static REAPER_STARTED: AtomicBool = AtomicBool::new(false);
    if REAPER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let keep_alive = match read_config(&app) {
                Ok(config) => config.keep_alive_minutes.unwrap_or(10),
                Err(_) => 10,
            };
            if keep_alive == 0 {
                continue;
            }
            let state = app.state::<SdcppState>();
            let mut guard = state.inner.lock().await;
            let idle_expired = guard
                .as_ref()
                .map(|proc| proc.last_used.elapsed() > Duration::from_secs(u64::from(keep_alive) * 60))
                .unwrap_or(false);
            if idle_expired {
                if let Some(mut proc) = guard.take() {
                    log_info(&app, "sdcpp", "Stopping idle sd-server");
                    let _ = proc.child.kill();
                    let _ = proc.child.wait();
                    emit_status(&app, "stopped", Some("idle".to_string()));
                }
            }
        }
    });
}

pub fn shutdown(app: &AppHandle) {
    if let Some(state) = app.try_state::<SdcppState>() {
        if let Ok(mut guard) = state.inner.try_lock() {
            if let Some(mut proc) = guard.take() {
                let _ = proc.child.kill();
                let _ = proc.child.wait();
            }
        } else {
            log_warn(app, "sdcpp", "Could not lock sd-server state during shutdown");
        }
    }
}

pub fn mark_used(app: &AppHandle) {
    if let Some(state) = app.try_state::<SdcppState>() {
        if let Ok(mut guard) = state.inner.try_lock() {
            if let Some(proc) = guard.as_mut() {
                proc.last_used = Instant::now();
            }
        }
    }
}

#[tauri::command]
pub fn sdcpp_get_config(app: AppHandle) -> Result<SdcppConfig, String> {
    read_config(&app)
}

#[tauri::command]
pub async fn sdcpp_set_config(
    app: AppHandle,
    config: SdcppConfig,
    state: State<'_, SdcppState>,
) -> Result<(), String> {
    let previous = read_config(&app)?;
    write_config(&app, &config)?;
    if config_fingerprint(&previous) != config_fingerprint(&config)
        || previous.port != config.port
    {
        stop_server_internal(&app, &state).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn sdcpp_get_status(
    app: AppHandle,
    state: State<'_, SdcppState>,
) -> Result<SdcppStatus, String> {
    let config = read_config(&app)?;
    let install = read_install_marker(&app);
    let mut guard = state.inner.lock().await;
    let (running, ready, port) = match guard.as_mut() {
        Some(proc) => {
            let alive = proc.child.try_wait().ok().flatten().is_none();
            if !alive {
                *guard = None;
                (false, false, None)
            } else {
                (true, proc.ready, Some(proc.port))
            }
        }
        None => (false, false, None),
    };
    Ok(SdcppStatus {
        binary_installed: install.is_some(),
        binary_tag: install.as_ref().map(|i| i.tag.clone()),
        binary_backend: install.map(|i| i.backend),
        model_configured: config.has_model(),
        server_running: running,
        server_ready: ready,
        base_url: port.map(|p| format!("http://127.0.0.1:{}", p)),
    })
}

#[tauri::command]
pub async fn sdcpp_start_server(app: AppHandle) -> Result<String, String> {
    ensure_server_running(&app).await
}

#[tauri::command]
pub async fn sdcpp_stop_server(
    app: AppHandle,
    state: State<'_, SdcppState>,
) -> Result<(), String> {
    stop_server_internal(&app, &state).await;
    Ok(())
}

#[tauri::command]
pub fn sdcpp_list_model_files(app: AppHandle) -> Result<Vec<HashMap<String, String>>, String> {
    let mut results = Vec::new();
    let gguf_dir = crate::utils::ensure_lettuce_dir(&app)?
        .join("models")
        .join("gguf");
    if let Ok(entries) = fs::read_dir(&gguf_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if !lower.ends_with(".gguf") && !lower.ends_with(".safetensors") {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let mut item = HashMap::new();
            item.insert("name".to_string(), name);
            item.insert("path".to_string(), path.to_string_lossy().to_string());
            item.insert("sizeBytes".to_string(), size.to_string());
            results.push(item);
        }
    }
    results.sort_by(|a, b| a.get("name").cmp(&b.get("name")));
    Ok(results)
}
