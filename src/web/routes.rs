use crate::{
    config::AppConfig,
    config_schema::{
        ConfigGuiUpdateRequest, ConfigSchema, apply_config_gui_values, build_config_schema,
    },
    config_store::SharedConfigStore,
    display::render::{
        PreviewFrame, dynamic_render_context_from_state_config, render_page_definition_to_frame,
    },
    led::{
        lab::{
            LED_LAB_BUS, LED_LAB_DEVICE_ADDRESS, LedLabCategory, LedLabCommandClass,
            LedLabConfidence, LedLabEntry, LedLabEntryDraft, LedLabRunnerSnapshot, LedLabStep,
            LedStudioModeConfig, OLED_DEVICE_ADDRESS, build_scan_steps, direct_single_color_steps,
            known_facts, validate_scan_bounds,
        },
        model::LedColor,
        strip::YahboomLedStrip,
    },
    page_catalog::{CatalogPageEntry, build_catalog, catalog_entry_for_page_id, parse_catalog_key},
    page_store::{
        DynamicSource, ImageMaskMode, ImportConflictPolicy, MonoColor, PAGE_ID_LIVE_INFO,
        PageDefinition, PageElement, PageSummary, SharedPageStore, TextSize, canonical_page_id,
        parse_page_definition_from_json_value,
    },
    published_store::{PublishedDisplaySpec, SharedPublishedStore},
    state::{AppState, DisplayMode, PreviewSnapshot, RuntimePage, ServerEvent, UiSnapshot},
    web::templates::{
        ConfigurationTemplate, DashboardTemplate, LedStudioTemplate, ProcessExplorerTemplate,
        StudioTemplate,
    },
};
use askama::Template;
use async_stream::stream;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        Html, IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    convert::Infallible,
    sync::Arc,
    time::Duration,
};
use tokio::{
    process::Command,
    sync::{RwLock, broadcast},
};
use tower_http::services::ServeDir;

#[derive(Clone)]
pub struct WebContext {
    pub state: Arc<RwLock<AppState>>,
    pub config_store: SharedConfigStore,
    pub page_store: SharedPageStore,
    pub published_store: SharedPublishedStore,
    pub led_lab_store: crate::led::lab::SharedLedLabStore,
    pub led_lab_runner: crate::led::lab::SharedLedLabRunner,
    pub event_tx: broadcast::Sender<ServerEvent>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub hostname: String,
    pub ip_addr: String,
    pub uptime_text: String,
    pub ram_percent_text: String,
    pub cpu_temp_text: String,
    pub active_page: String,
}

#[derive(Debug, Serialize)]
pub struct DisplayPageResponse {
    pub active_page: String,
}

#[derive(Debug, Serialize)]
pub struct DisplayModeResponse {
    pub display_mode: String,
    pub rotation_interval_ms: u64,
    pub rotation_queue: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RotationIntervalParams {
    pub ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct SetRuntimePageRequest {
    #[serde(alias = "page_ref", deserialize_with = "deserialize_request_page_id")]
    pub page_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RequestPageIdWire {
    Id(String),
    Legacy(RequestLegacyPageRef),
}

#[derive(Debug, Deserialize)]
struct RequestLegacyPageRef {
    value: String,
}

fn deserialize_request_page_id<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = RequestPageIdWire::deserialize(deserializer)?;
    Ok(match value {
        RequestPageIdWire::Id(id) => id,
        RequestPageIdWire::Legacy(legacy) => legacy.value,
    })
}

fn normalized_element_name(raw: Option<String>) -> Option<String> {
    raw.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn default_lab_bus() -> u8 {
    LED_LAB_BUS
}

fn default_lab_led_address() -> u8 {
    LED_LAB_DEVICE_ADDRESS
}

#[derive(Debug, Deserialize)]
pub struct CreatePageQuery {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct RenamePageQuery {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct RekeyPageRequest {
    pub new_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ImportPageQuery {
    pub conflict: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddStaticTextRequest {
    pub x: i32,
    pub y: i32,
    pub text: String,
    pub size: Option<TextSize>,
    pub text_height_px: Option<u32>,
    pub color: Option<MonoColor>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddDynamicTextRequest {
    pub x: i32,
    pub y: i32,
    pub source: DynamicSource,
    pub prefix: Option<String>,
    pub max_chars: Option<usize>,
    pub size: Option<TextSize>,
    pub text_height_px: Option<u32>,
    pub color: Option<MonoColor>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddRectRequest {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    #[serde(default)]
    pub filled: bool,
    pub fill: Option<MonoColor>,
    pub stroke: Option<MonoColor>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddImageRequest {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub source: String,
    pub mask_mode: Option<ImageMaskMode>,
    pub threshold: Option<u8>,
    pub foreground: Option<MonoColor>,
    pub background: Option<MonoColor>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddLineRequest {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    pub color: Option<MonoColor>,
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StudioPageListResponse {
    pub pages: Vec<PageSummary>,
}

#[derive(Debug, Serialize)]
pub struct LedRuntimeResponse {
    pub playing: bool,
    pub controller_mode_enabled: bool,
    pub controller_mode: LedControllerModeResponse,
}

#[derive(Debug, Serialize)]
pub struct LedControllerModeResponse {
    pub mode: u8,
    pub speed: u8,
    pub color_index: u8,
}

#[derive(Debug, Serialize)]
pub struct LedStateResponse {
    pub runtime: LedRuntimeResponse,
    pub hardware_led_count: u16,
    pub live_frame: Vec<LedColor>,
    pub execution_mode: String,
    pub frame_is_estimated: bool,
    pub offloaded: Option<LedOffloadedResponse>,
    pub physically_synced: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LedOffloadedResponse {
    pub mode: u8,
    pub speed: u8,
    pub color_index: u8,
    pub source_effect_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ToggleParams {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct FanAutoThresholdParams {
    pub c: u8,
}

#[derive(Debug, Deserialize)]
pub struct SetLedControllerModeRequest {
    pub enabled: bool,
    pub mode: Option<u8>,
    pub speed: Option<u8>,
    pub color_index: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct LedDirectPixelRequest {
    pub index: u8,
    pub hex: String,
}

#[derive(Debug, Serialize)]
pub struct LedLabStateResponse {
    pub known_facts: crate::led::lab::LedLabKnownFacts,
    pub studio_modes: Vec<LedStudioModeConfig>,
    pub entries: Vec<LedLabEntry>,
    pub categories: Vec<LedLabCategoryCoverage>,
    pub register_groups: Vec<LedLabRegisterCoverage>,
    pub runner: LedLabRunnerSnapshot,
    pub defaults: LedLabDefaults,
}

#[derive(Debug, Serialize)]
pub struct PowerStateResponse {
    pub standby_active: bool,
    pub led_requested_on: bool,
    pub led_effective_on: bool,
    pub fan_requested_on: bool,
    pub fan_effective_on: bool,
    pub fan_auto_forced_by_temp: bool,
    pub fan_explicit_off_temp_warning: bool,
    pub fan_last_error: Option<String>,
    pub fan_auto_on_temp_c: u8,
}

#[derive(Debug, Serialize)]
pub struct LedLabDefaults {
    pub bus: u8,
    pub led_device_address: u8,
    pub oled_device_address: u8,
}

#[derive(Debug, Serialize)]
pub struct LedLabCategoryCoverage {
    pub id: String,
    pub label: String,
    pub total_entries: usize,
    pub confirmed: usize,
    pub likely: usize,
    pub unknown: usize,
    pub conflicting: usize,
}

#[derive(Debug, Serialize)]
pub struct LedLabRegisterCoverage {
    pub id: String,
    pub title: String,
    pub category_id: String,
    pub category_label: String,
    pub command_class: String,
    pub register: u8,
    pub range_start: u8,
    pub range_end: u8,
    pub total_slots: usize,
    pub known_slots: usize,
    pub unknown_slots: usize,
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct ProcessListResponse {
    pub processes: Vec<ProcessInfo>,
    pub total: usize,
    pub current_pid: u32,
    pub available_users: Vec<String>,
    pub totals: ProcessTotals,
}

#[derive(Debug, Serialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub user: String,
    pub cpu_percent: f32,
    pub mem_percent: f32,
    pub rss_kib: u64,
    pub vsz_kib: u64,
    pub rss_human: String,
    pub vsz_human: String,
    pub state: String,
    pub elapsed: String,
    pub name: String,
    pub display_name: String,
    pub command: String,
    pub bound_endpoints: Vec<String>,
    pub can_kill: bool,
    pub protected_reason: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ProcessTotals {
    pub cpu_percent_sum: f32,
    pub mem_percent_sum: f32,
    pub rss_kib_sum: u64,
    pub vsz_kib_sum: u64,
    pub rss_human_sum: String,
    pub vsz_human_sum: String,
}

#[derive(Debug, Deserialize)]
pub struct KillProcessQuery {
    pub signal: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LedLabRunRequest {
    pub label: String,
    #[serde(default = "default_lab_bus")]
    pub bus: u8,
    #[serde(default = "default_lab_led_address")]
    pub device_address: u8,
    pub steps: Vec<LedLabStep>,
}

#[derive(Debug, Deserialize)]
pub struct LedLabScanRequest {
    #[serde(default = "default_lab_bus")]
    pub bus: u8,
    #[serde(default = "default_lab_led_address")]
    pub device_address: u8,
    pub register_start: u8,
    pub register_end: u8,
    pub value_start: u8,
    pub value_end: u8,
    #[serde(default)]
    pub delay_ms: u64,
    #[serde(default)]
    pub allow_larger: bool,
    pub label: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LedLabDirectSingleColorRequest {
    pub index: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    #[serde(default)]
    pub delay_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct StudioCatalogResponse {
    pub pages: Vec<CatalogPageEntry>,
}

#[derive(Debug, Serialize)]
pub struct StudioPageDetailResponse {
    pub page: CatalogPageEntry,
    pub definition: Option<PageDefinition>,
}

#[derive(Debug, Serialize)]
pub struct SystemConfigSchemaResponse {
    pub schema: ConfigSchema,
    pub raw_toml: String,
}

pub fn router(ctx: WebContext) -> Router {
    Router::new()
        .nest_service("/assets", ServeDir::new("assets"))
        .route("/", get(dashboard))
        .route("/processes", get(process_explorer))
        .route("/studio", get(studio))
        .route("/led", get(led_studio))
        .route("/configuration", get(configuration))
        .route("/api/health", get(health))
        .route("/api/processes", get(get_processes))
        .route("/api/processes/{pid}/kill", post(kill_process))
        .route("/api/metrics", get(metrics))
        .route("/api/events", get(events))
        .route("/api/studio/events/{id}", get(studio_events))
        .route("/api/display/page", get(get_display_page))
        .route("/api/display/page/{name}", post(set_display_page))
        .route("/api/display/runtime_page", post(set_runtime_page_override))
        .route("/api/display/rotation/enable", post(enable_rotation))
        .route("/api/display/rotation/disable", post(disable_rotation))
        .route(
            "/api/display/rotation/interval",
            post(set_temporary_rotation_interval),
        )
        .route(
            "/api/display/rotation/resume_published",
            post(resume_published_rotation),
        )
        .route("/api/display/mode", get(get_display_mode))
        .route("/api/config/rotation_interval", post(set_rotation_interval))
        .route("/api/system/config/schema", get(get_system_config_schema))
        .route("/api/system/config/gui", post(save_system_config_gui))
        .route("/api/system/config/raw", get(get_system_config_raw))
        .route("/api/system/config/raw", post(save_system_config_raw))
        .route("/api/publish/spec", get(get_published_spec))
        .route("/api/publish/spec", post(save_published_spec))
        .route("/api/studio/catalog", get(get_studio_catalog))
        .route(
            "/api/studio/catalog/page/{key}",
            get(get_studio_catalog_page),
        )
        .route("/api/studio/pages", get(list_studio_pages))
        .route("/api/studio/pages/create", post(create_studio_page))
        .route("/api/studio/pages/{id}", get(get_studio_page))
        .route("/api/studio/pages/{id}/export", get(export_studio_page))
        .route("/api/studio/pages/import", post(import_studio_page))
        .route("/api/studio/pages/{id}/rename", post(rename_studio_page))
        .route("/api/studio/pages/{id}/rekey", post(rekey_studio_page))
        .route("/api/studio/pages/{id}/delete", post(delete_studio_page))
        .route("/api/studio/pages/{id}/replace", post(replace_studio_page))
        .route(
            "/api/studio/pages/{id}/apply",
            post(apply_studio_page_to_live),
        )
        .route(
            "/api/studio/pages/{id}/elements/static_text",
            post(add_static_text_element),
        )
        .route(
            "/api/studio/pages/{id}/elements/dynamic_text",
            post(add_dynamic_text_element),
        )
        .route(
            "/api/studio/pages/{id}/elements/rect",
            post(add_rect_element),
        )
        .route(
            "/api/studio/pages/{id}/elements/image",
            post(add_image_element),
        )
        .route(
            "/api/studio/pages/{id}/elements/line",
            post(add_line_element),
        )
        .route(
            "/api/studio/pages/{id}/elements/{index}/delete",
            post(delete_studio_element),
        )
        .route("/api/led/state", get(get_led_state))
        .route("/api/led/runtime", get(get_led_runtime))
        .route(
            "/api/led/runtime/controller_mode",
            post(set_led_controller_mode),
        )
        .route("/api/led/runtime/direct_pixel", post(set_led_direct_pixel))
        .route("/api/led/runtime/play", post(play_led_runtime))
        .route("/api/led/runtime/pause", post(pause_led_runtime))
        .route("/api/led/runtime/stop", post(stop_led_runtime))
        .route("/api/power", get(get_power_state))
        .route("/api/power/led", post(set_led_requested_state))
        .route("/api/power/fan", post(set_fan_requested_state))
        .route("/api/power/standby", post(set_standby_state))
        .route(
            "/api/power/fan/auto_threshold",
            post(set_fan_auto_threshold),
        )
        .route("/api/led/lab/state", get(get_led_lab_state))
        .route("/api/led/lab/run", post(run_led_lab_steps))
        .route(
            "/api/led/lab/direct/single_color/test",
            post(run_led_lab_direct_single_color),
        )
        .route("/api/led/lab/scan", post(run_led_lab_scan))
        .route("/api/led/lab/entries/save", post(save_led_lab_entry))
        .route("/api/led/lab/entries/{id}/run", post(run_led_lab_entry))
        .route(
            "/api/led/lab/entries/{id}/delete",
            post(delete_led_lab_entry),
        )
        .route("/api/led/lab/abort", post(abort_led_lab_run))
        .with_state(ctx)
}

async fn dashboard(State(ctx): State<WebContext>) -> impl IntoResponse {
    let (current_page, display_mode, rotation_interval_ms) = {
        let state = ctx.state.read().await;
        (
            state.active_page.label(),
            state.display_mode.as_str().to_string(),
            state.rotation_interval_ms,
        )
    };

    let (display_width, display_height, fan_auto_on_temp_c) = {
        let store = ctx.config_store.read().await;
        (
            store.config.display.width as usize,
            store.config.display.height as usize,
            store.config.led.fan_auto_on_temp_c,
        )
    };

    let preview_scale = 6usize;
    let page_catalog_json = {
        let store = ctx.page_store.read().await;
        let catalog = build_catalog(&store);
        serde_json::to_string(&catalog).unwrap_or_else(|_| "[]".to_string())
    };
    let published_spec_json = {
        let store = ctx.published_store.read().await;
        serde_json::to_string(&store.spec).unwrap_or_else(|_| "{}".to_string())
    };
    let template = DashboardTemplate {
        page_title: "SOLSTICE Panel".to_string(),
        preview_css_width: display_width * preview_scale,
        preview_css_height: display_height * preview_scale,
        display_width,
        display_height,
        fan_auto_on_temp_c,
        current_page,
        display_mode,
        rotation_interval_ms,
        page_catalog_json,
        published_spec_json,
    };

    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("template render failed: {err}"),
        )
            .into_response(),
    }
}

async fn process_explorer() -> impl IntoResponse {
    let template = ProcessExplorerTemplate {
        page_title: "Process Explorer".to_string(),
    };

    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("template render failed: {err}"),
        )
            .into_response(),
    }
}

async fn studio(State(ctx): State<WebContext>) -> impl IntoResponse {
    let (page_catalog, initial_catalog_key) = {
        let store = ctx.page_store.read().await;
        let catalog = build_catalog(&store);
        let initial = catalog
            .first()
            .map(|p| p.key.clone())
            .unwrap_or_else(|| PAGE_ID_LIVE_INFO.to_string());
        (catalog, initial)
    };

    let page_catalog_json =
        serde_json::to_string(&page_catalog).unwrap_or_else(|_| "[]".to_string());

    let published_spec_json = {
        let store = ctx.published_store.read().await;
        match store.spec_json_pretty() {
            Ok(s) => s,
            Err(_) => "{}".to_string(),
        }
    };

    let (display_width, display_height) = {
        let store = ctx.config_store.read().await;
        (
            store.config.display.width as usize,
            store.config.display.height as usize,
        )
    };

    let preview_scale = 6usize;
    let template = StudioTemplate {
        page_title: "OLED Studio".to_string(),
        preview_css_width: display_width * preview_scale,
        preview_css_height: display_height * preview_scale,
        display_width,
        display_height,
        initial_catalog_key,
        page_catalog_json,
        published_spec_json,
    };

    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("template render failed: {err}"),
        )
            .into_response(),
    }
}

async fn led_studio(State(ctx): State<WebContext>) -> impl IntoResponse {
    let bootstrap = build_led_lab_state(&ctx).await;
    let lab_bootstrap_json = serde_json::to_string(&bootstrap).unwrap_or_else(|_| "{}".to_string());

    let template = LedStudioTemplate {
        page_title: "LED Studio".to_string(),
        lab_bootstrap_json,
    };

    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("template render failed: {err}"),
        )
            .into_response(),
    }
}

async fn configuration(State(ctx): State<WebContext>) -> impl IntoResponse {
    let (config_schema_json, system_config_toml) = {
        let store = ctx.config_store.read().await;
        let schema = build_config_schema(&store.config);
        let schema_json =
            serde_json::to_string(&schema).unwrap_or_else(|_| "{\"sections\":[]}".to_string());
        let raw_toml = store.config_toml_pretty().unwrap_or_default();
        (schema_json, raw_toml)
    };

    let template = ConfigurationTemplate {
        page_title: "Configuration".to_string(),
        config_schema_json,
        system_config_toml,
    };

    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("template render failed: {err}"),
        )
            .into_response(),
    }
}

async fn health() -> impl IntoResponse {
    Json(HealthResponse { status: "ok" })
}

fn kib_human(kib: u64) -> String {
    let bytes = kib.saturating_mul(1024);
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let as_f64 = bytes as f64;
    if as_f64 >= GIB {
        format!("{:.2} GiB", as_f64 / GIB)
    } else if as_f64 >= MIB {
        format!("{:.1} MiB", as_f64 / MIB)
    } else if as_f64 >= KIB {
        format!("{:.1} KiB", as_f64 / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn display_process_name(name: &str, command: &str) -> String {
    if name == "solstice-panel" || command.contains("solstice-panel") {
        "SOLSTICE Panel".to_string()
    } else if name == "systemd" {
        "System Manager".to_string()
    } else if name == "sshd" {
        "SSH Daemon".to_string()
    } else if name == "bash" {
        "Bash Shell".to_string()
    } else if name == "python3" {
        "Python 3".to_string()
    } else {
        name.to_string()
    }
}

fn parse_ps_output(raw: &str) -> Vec<ProcessInfo> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let pid = parts.next().and_then(|v| v.parse::<u32>().ok());
        let ppid = parts.next().and_then(|v| v.parse::<u32>().ok());
        let user = parts.next().map(str::to_string);
        let cpu_percent = parts.next().and_then(|v| v.parse::<f32>().ok());
        let mem_percent = parts.next().and_then(|v| v.parse::<f32>().ok());
        let rss_kib = parts.next().and_then(|v| v.parse::<u64>().ok());
        let vsz_kib = parts.next().and_then(|v| v.parse::<u64>().ok());
        let state = parts.next().map(str::to_string);
        let elapsed = parts.next().map(str::to_string);
        let name = parts.next().map(str::to_string);
        let command = parts.collect::<Vec<_>>().join(" ");

        let Some(pid) = pid else { continue };
        let Some(ppid) = ppid else { continue };
        let Some(user) = user else { continue };
        let Some(cpu_percent) = cpu_percent else {
            continue;
        };
        let Some(mem_percent) = mem_percent else {
            continue;
        };
        let Some(rss_kib) = rss_kib else { continue };
        let Some(vsz_kib) = vsz_kib else { continue };
        let Some(state) = state else { continue };
        let Some(elapsed) = elapsed else { continue };
        let Some(name) = name else { continue };
        let display_name = display_process_name(&name, &command);

        out.push(ProcessInfo {
            pid,
            ppid,
            user,
            cpu_percent,
            mem_percent,
            rss_kib,
            vsz_kib,
            rss_human: kib_human(rss_kib),
            vsz_human: kib_human(vsz_kib),
            state,
            elapsed,
            name,
            display_name,
            command: if command.is_empty() {
                String::new()
            } else {
                command
            },
            bound_endpoints: Vec::new(),
            can_kill: false,
            protected_reason: None,
        });
    }

    out
}

fn parse_ps_aux_totals(raw: &str) -> Option<ProcessTotals> {
    let mut cpu_percent_sum = 0.0f64;
    let mut mem_percent_sum = 0.0f64;
    let mut rss_kib_sum = 0u64;
    let mut vsz_kib_sum = 0u64;
    let mut parsed_lines = 0usize;

    for line in raw.lines() {
        let cols = line.split_whitespace().collect::<Vec<_>>();
        if cols.len() < 6 {
            continue;
        }
        let Ok(cpu_percent) = cols[2].parse::<f64>() else {
            continue;
        };
        let Ok(mem_percent) = cols[3].parse::<f64>() else {
            continue;
        };
        let Ok(vsz_kib) = cols[4].parse::<u64>() else {
            continue;
        };
        let Ok(rss_kib) = cols[5].parse::<u64>() else {
            continue;
        };

        parsed_lines += 1;
        cpu_percent_sum += cpu_percent;
        mem_percent_sum += mem_percent;
        rss_kib_sum = rss_kib_sum.saturating_add(rss_kib);
        vsz_kib_sum = vsz_kib_sum.saturating_add(vsz_kib);
    }

    if parsed_lines == 0 {
        return None;
    }

    Some(ProcessTotals {
        cpu_percent_sum: cpu_percent_sum as f32,
        mem_percent_sum: mem_percent_sum as f32,
        rss_kib_sum,
        vsz_kib_sum,
        rss_human_sum: kib_human(rss_kib_sum),
        vsz_human_sum: kib_human(vsz_kib_sum),
    })
}

fn process_totals_from_list(processes: &[ProcessInfo]) -> ProcessTotals {
    let mut cpu_percent_sum = 0.0f64;
    let mut mem_percent_sum = 0.0f64;
    let mut rss_kib_sum = 0u64;
    let mut vsz_kib_sum = 0u64;

    for process in processes {
        cpu_percent_sum += process.cpu_percent as f64;
        mem_percent_sum += process.mem_percent as f64;
        rss_kib_sum = rss_kib_sum.saturating_add(process.rss_kib);
        vsz_kib_sum = vsz_kib_sum.saturating_add(process.vsz_kib);
    }

    ProcessTotals {
        cpu_percent_sum: cpu_percent_sum as f32,
        mem_percent_sum: mem_percent_sum as f32,
        rss_kib_sum,
        vsz_kib_sum,
        rss_human_sum: kib_human(rss_kib_sum),
        vsz_human_sum: kib_human(vsz_kib_sum),
    }
}

fn extract_pid_values(raw: &str) -> Vec<u32> {
    let mut pids = HashSet::new();
    let mut cursor = raw;

    while let Some(start) = cursor.find("pid=") {
        let after = &cursor[start + 4..];
        let digits = after
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if let Ok(pid) = digits.parse::<u32>() {
            pids.insert(pid);
        }
        if digits.is_empty() {
            if after.is_empty() {
                break;
            }
            cursor = &after[1..];
        } else {
            cursor = &after[digits.len()..];
        }
    }

    let mut out = pids.into_iter().collect::<Vec<_>>();
    out.sort_unstable();
    out
}

fn parse_ss_bound_output(raw: &str) -> HashMap<u32, Vec<String>> {
    let mut map: HashMap<u32, HashSet<String>> = HashMap::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let cols = trimmed.split_whitespace().collect::<Vec<_>>();
        if cols.len() < 7 {
            continue;
        }

        let local_endpoint = cols[4].to_string();
        let process_blob = cols[6..].join(" ");
        if process_blob.is_empty() {
            continue;
        }

        for pid in extract_pid_values(&process_blob) {
            map.entry(pid).or_default().insert(local_endpoint.clone());
        }
    }

    let mut out = HashMap::new();
    for (pid, endpoints) in map {
        let mut endpoints_vec = endpoints.into_iter().collect::<Vec<_>>();
        endpoints_vec.sort();
        out.insert(pid, endpoints_vec);
    }
    out
}

async fn fetch_bound_endpoints_by_pid() -> HashMap<u32, Vec<String>> {
    let output = Command::new("ss")
        .arg("-H")
        .arg("-l")
        .arg("-n")
        .arg("-t")
        .arg("-u")
        .arg("-a")
        .arg("-p")
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            let raw = String::from_utf8_lossy(&out.stdout);
            parse_ss_bound_output(&raw)
        }
        _ => HashMap::new(),
    }
}

fn protected_process_reason(process: &ProcessInfo, current_pid: u32) -> Option<String> {
    if process.pid <= 1 {
        return Some("PID 1 and lower are protected".to_string());
    }
    if process.pid == current_pid {
        return Some("current SOLSTICE Panel process is protected".to_string());
    }

    let name_lower = process.name.to_ascii_lowercase();
    let command_lower = process.command.to_ascii_lowercase();
    let user_lower = process.user.to_ascii_lowercase();

    if name_lower == "solstice-panel" || command_lower.contains("solstice-panel") {
        return Some("SOLSTICE Panel processes are protected".to_string());
    }

    if user_lower == "root" {
        return Some("root-owned processes are protected in Process Explorer".to_string());
    }

    if process.command.starts_with('[') && process.command.ends_with(']') {
        return Some("kernel-managed threads are protected".to_string());
    }

    const CRITICAL_EXACT: &[&str] = &[
        "init",
        "systemd",
        "systemd-journald",
        "systemd-logind",
        "systemd-resolved",
        "systemd-timesyncd",
        "systemd-udevd",
        "dbus-daemon",
        "networkmanager",
        "sshd",
        "cron",
        "rsyslogd",
        "agetty",
        "polkitd",
        "udisksd",
        "containerd",
        "dockerd",
        "kubelet",
    ];
    if CRITICAL_EXACT.contains(&name_lower.as_str()) {
        return Some("critical system process is protected".to_string());
    }

    const CRITICAL_PREFIXES: &[&str] = &[
        "kthreadd",
        "ksoftirqd",
        "kworker",
        "rcu",
        "migration/",
        "watchdog",
        "irq/",
        "oom_",
        "systemd-",
    ];
    if CRITICAL_PREFIXES
        .iter()
        .any(|prefix| name_lower.starts_with(prefix))
    {
        return Some("critical kernel/system process is protected".to_string());
    }

    if command_lower.contains("/sbin/init") || command_lower.contains("/lib/systemd/systemd") {
        return Some("system init process is protected".to_string());
    }

    None
}

fn apply_process_protection(processes: &mut [ProcessInfo], current_pid: u32) {
    for process in processes {
        let protected_reason = protected_process_reason(process, current_pid);
        process.can_kill = protected_reason.is_none();
        process.protected_reason = protected_reason;
    }
}

fn available_users_for_processes(processes: &[ProcessInfo]) -> Vec<String> {
    let mut users = BTreeSet::new();
    for process in processes {
        users.insert(process.user.clone());
    }
    users.into_iter().collect()
}

async fn fetch_ps_aux_totals() -> Option<ProcessTotals> {
    let output = Command::new("ps").arg("-aux").output().await.ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    parse_ps_aux_totals(&raw)
}

async fn fetch_processes_snapshot() -> Result<Vec<ProcessInfo>, String> {
    let output = Command::new("ps")
        .arg("-eo")
        .arg("pid=,ppid=,user=,%cpu=,%mem=,rss=,vsz=,stat=,etime=,comm=,args=")
        .arg("--sort=-%cpu")
        .output()
        .await
        .map_err(|err| format!("failed to execute ps: {err}"))?;

    if !output.status.success() {
        return Err("ps command failed".to_string());
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut processes = parse_ps_output(&raw);
    let endpoints_by_pid = fetch_bound_endpoints_by_pid().await;
    for process in &mut processes {
        if let Some(endpoints) = endpoints_by_pid.get(&process.pid) {
            process.bound_endpoints = endpoints.clone();
        }
    }

    Ok(processes)
}

async fn get_processes() -> impl IntoResponse {
    let current_pid = std::process::id();
    let mut processes = match fetch_processes_snapshot().await {
        Ok(processes) => processes,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": err})),
            )
                .into_response();
        }
    };
    apply_process_protection(&mut processes, current_pid);

    let available_users = available_users_for_processes(&processes);
    let totals = fetch_ps_aux_totals()
        .await
        .unwrap_or_else(|| process_totals_from_list(&processes));

    Json(ProcessListResponse {
        total: processes.len(),
        processes,
        current_pid,
        available_users,
        totals,
    })
    .into_response()
}

async fn kill_process(
    Path(pid): Path<u32>,
    Query(query): Query<KillProcessQuery>,
) -> impl IntoResponse {
    let signal = query
        .signal
        .as_deref()
        .unwrap_or("TERM")
        .trim()
        .to_uppercase();
    let allowed = ["TERM", "KILL", "INT", "HUP", "QUIT"];
    if !allowed.contains(&signal.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": "unsupported signal"})),
        )
            .into_response();
    }

    let current_pid = std::process::id();
    let processes = match fetch_processes_snapshot().await {
        Ok(processes) => processes,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": err})),
            )
                .into_response();
        }
    };
    let process = match processes.iter().find(|p| p.pid == pid) {
        Some(process) => process,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"ok": false, "error": format!("PID {pid} no longer exists")})),
            )
                .into_response();
        }
    };

    if let Some(reason) = protected_process_reason(process, current_pid) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": reason})),
        )
            .into_response();
    }

    let output = match Command::new("kill")
        .arg("-s")
        .arg(signal.as_str())
        .arg(pid.to_string())
        .output()
        .await
    {
        Ok(output) => output,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": format!("failed to execute kill: {err}")})),
            )
                .into_response();
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = stderr.trim();
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "ok": false,
                "error": if msg.is_empty() { "kill command failed" } else { msg }
            })),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "pid": pid,
            "signal": signal
        })),
    )
        .into_response()
}

async fn metrics(State(ctx): State<WebContext>) -> impl IntoResponse {
    let state = ctx.state.read().await;

    Json(MetricsResponse {
        hostname: state.metrics.hostname.clone(),
        ip_addr: state.metrics.ip_addr.clone(),
        uptime_text: state.metrics.uptime_text.clone(),
        ram_percent_text: state.metrics.ram_percent_text.clone(),
        cpu_temp_text: state.metrics.cpu_temp_text.clone(),
        active_page: state.active_page.label(),
    })
}

async fn events(State(ctx): State<WebContext>) -> impl IntoResponse {
    let (initial_status, initial_preview) = {
        let state = ctx.state.read().await;
        let config = ctx.config_store.read().await.config.clone();

        let status = UiSnapshot::from_state(&state);
        let preview = build_live_preview_snapshot(&ctx, &state, &config).await;

        (status, preview)
    };

    let mut rx = ctx.event_tx.subscribe();

    let stream = stream! {
        if let Ok(event) = Event::default().event("status").json_data(initial_status) {
            yield Ok::<Event, Infallible>(event);
        }

        if let Ok(event) = Event::default().event("preview").json_data(initial_preview) {
            yield Ok::<Event, Infallible>(event);
        }

        loop {
            match rx.recv().await {
                Ok(ServerEvent::Status(snapshot)) => {
                    if let Ok(event) = Event::default().event("status").json_data(snapshot) {
                        yield Ok::<Event, Infallible>(event);
                    }
                }
                Ok(ServerEvent::Preview(snapshot)) => {
                    if let Ok(event) = Event::default().event("preview").json_data(snapshot) {
                        yield Ok::<Event, Infallible>(event);
                    }
                }
                Ok(ServerEvent::Invalidate) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

async fn studio_events(Path(id): Path<String>, State(ctx): State<WebContext>) -> impl IntoResponse {
    let initial_preview = build_studio_preview_snapshot(&ctx, &id).await;
    let mut rx = ctx.event_tx.subscribe();

    let stream = stream! {
        if let Ok(event) = Event::default().event("draft_preview").json_data(initial_preview.clone()) {
            yield Ok::<Event, Infallible>(event);
        }

        loop {
            match rx.recv().await {
                Ok(_) => {
                    let preview = build_studio_preview_snapshot(&ctx, &id).await;
                    if let Ok(event) = Event::default().event("draft_preview").json_data(preview) {
                        yield Ok::<Event, Infallible>(event);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let preview = build_studio_preview_snapshot(&ctx, &id).await;
                    if let Ok(event) = Event::default().event("draft_preview").json_data(preview) {
                        yield Ok::<Event, Infallible>(event);
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

async fn get_display_page(State(ctx): State<WebContext>) -> impl IntoResponse {
    let state = ctx.state.read().await;
    Json(DisplayPageResponse {
        active_page: state.active_page.label(),
    })
}

async fn set_display_page(
    Path(requested_page_id): Path<String>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    let page_id = resolve_existing_page_id(&ctx, &requested_page_id).await;
    let Some(page_id) = page_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": format!("unknown page '{}'", requested_page_id)})),
        )
            .into_response();
    };

    {
        let mut state = ctx.state.write().await;
        state.active_page = RuntimePage::from_id(page_id.clone());
        state.display_mode = DisplayMode::Manual;
    }
    let (snapshot, preview) = emit_status_preview(&ctx).await;

    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "active_page": page_id,
            "display_mode": "manual",
            "snapshot": snapshot,
            "preview": preview
        })),
    )
        .into_response()
}

async fn set_runtime_page_override(
    State(ctx): State<WebContext>,
    Json(req): Json<SetRuntimePageRequest>,
) -> impl IntoResponse {
    let page_id = resolve_existing_page_id(&ctx, &req.page_id).await;
    let Some(page_id) = page_id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": format!("unknown page '{}'", req.page_id)})),
        )
            .into_response();
    };

    {
        let mut state = ctx.state.write().await;
        state.active_page = RuntimePage::from_id(page_id);
        state.display_mode = DisplayMode::Manual;
    }
    let (snapshot, preview) = emit_status_preview(&ctx).await;

    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "snapshot": snapshot,
            "preview": preview
        })),
    )
        .into_response()
}

async fn get_display_mode(State(ctx): State<WebContext>) -> impl IntoResponse {
    let state = ctx.state.read().await;

    Json(DisplayModeResponse {
        display_mode: state.display_mode.as_str().to_string(),
        rotation_interval_ms: state.rotation_interval_ms,
        rotation_queue: state.rotation_queue.iter().map(|p| p.label()).collect(),
    })
}

async fn enable_rotation(State(ctx): State<WebContext>) -> impl IntoResponse {
    {
        let mut state = ctx.state.write().await;
        state.display_mode = DisplayMode::Rotating;
    }
    let (snapshot, preview) = emit_status_preview(&ctx).await;

    (
        StatusCode::OK,
        Json(json!({"ok": true, "snapshot": snapshot, "preview": preview})),
    )
        .into_response()
}

async fn disable_rotation(State(ctx): State<WebContext>) -> impl IntoResponse {
    {
        let mut state = ctx.state.write().await;
        state.display_mode = DisplayMode::Manual;
    }
    let (snapshot, preview) = emit_status_preview(&ctx).await;

    (
        StatusCode::OK,
        Json(json!({"ok": true, "snapshot": snapshot, "preview": preview})),
    )
        .into_response()
}

async fn resume_published_rotation(State(ctx): State<WebContext>) -> impl IntoResponse {
    let spec = ctx.published_store.read().await.spec.clone();

    {
        let mut state = ctx.state.write().await;
        spec.apply_to_runtime_state(&mut state);
    }
    let (snapshot, preview) = emit_status_preview(&ctx).await;

    (
        StatusCode::OK,
        Json(json!({"ok": true, "snapshot": snapshot, "preview": preview})),
    )
        .into_response()
}

async fn set_temporary_rotation_interval(
    Query(params): Query<RotationIntervalParams>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    if params.ms < 250 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": "rotation interval must be at least 250 ms"})),
        )
            .into_response();
    }

    {
        let mut state = ctx.state.write().await;
        state.rotation_interval_ms = params.ms;
    }

    let (snapshot, preview) = emit_status_preview(&ctx).await;

    (
        StatusCode::OK,
        Json(json!({"ok": true, "snapshot": snapshot, "preview": preview})),
    )
        .into_response()
}

async fn set_rotation_interval(
    Query(params): Query<RotationIntervalParams>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    {
        let mut published = ctx.published_store.write().await;
        published.spec.rotation_interval_ms = params.ms;
        if let Err(err) = published.save() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }
    }

    {
        let mut state = ctx.state.write().await;
        state.rotation_interval_ms = params.ms;
    }

    let (snapshot, preview) = emit_status_preview(&ctx).await;

    (
        StatusCode::OK,
        Json(json!({"ok": true, "snapshot": snapshot, "preview": preview})),
    )
        .into_response()
}

async fn get_system_config_schema(State(ctx): State<WebContext>) -> impl IntoResponse {
    match load_system_config_schema_and_raw(&ctx).await {
        Ok((schema, raw_toml)) => {
            Json(SystemConfigSchemaResponse { schema, raw_toml }).into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": err})),
        )
            .into_response(),
    }
}

async fn save_system_config_gui(
    State(ctx): State<WebContext>,
    Json(req): Json<ConfigGuiUpdateRequest>,
) -> impl IntoResponse {
    let updated = {
        let store = ctx.config_store.read().await;
        let mut updated = store.config.clone();

        if let Err(err) = apply_config_gui_values(&mut updated, &req.values) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }

        updated
    };

    let save_result = match persist_system_config(&ctx, updated).await {
        Ok(result) => result,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": err})),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "schema": save_result.schema,
            "raw_toml": save_result.raw_toml,
            "snapshot": save_result.snapshot,
            "preview": save_result.preview
        })),
    )
        .into_response()
}

async fn get_system_config_raw(State(ctx): State<WebContext>) -> impl IntoResponse {
    match load_system_config_raw_toml(&ctx).await {
        Ok(raw) => (StatusCode::OK, raw).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to serialize config: {err}"),
        )
            .into_response(),
    }
}

async fn save_system_config_raw(State(ctx): State<WebContext>, body: String) -> impl IntoResponse {
    let parsed: AppConfig = match toml::from_str(&body) {
        Ok(cfg) => cfg,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": format!("invalid TOML: {err}")})),
            )
                .into_response();
        }
    };

    if let Err(err) = parsed.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": err.to_string()})),
        )
            .into_response();
    }

    let save_result = match persist_system_config(&ctx, parsed).await {
        Ok(result) => result,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": err})),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "schema": save_result.schema,
            "raw_toml": save_result.raw_toml,
            "snapshot": save_result.snapshot,
            "preview": save_result.preview
        })),
    )
        .into_response()
}

struct SystemConfigSaveResult {
    schema: ConfigSchema,
    raw_toml: String,
    snapshot: UiSnapshot,
    preview: PreviewSnapshot,
}

async fn persist_system_config(
    ctx: &WebContext,
    config: AppConfig,
) -> Result<SystemConfigSaveResult, String> {
    {
        let mut store = ctx.config_store.write().await;
        store
            .replace_config(config)
            .map_err(|err| err.to_string())?;
    }

    let (schema, raw_toml) = load_system_config_schema_and_raw(ctx).await?;
    let (snapshot, preview) = emit_invalidate_status_preview(ctx).await;

    Ok(SystemConfigSaveResult {
        schema,
        raw_toml,
        snapshot,
        preview,
    })
}

async fn load_system_config_schema_and_raw(
    ctx: &WebContext,
) -> Result<(ConfigSchema, String), String> {
    let store = ctx.config_store.read().await;
    let schema = build_config_schema(&store.config);
    let raw_toml = store.config_toml_pretty().map_err(|err| err.to_string())?;
    Ok((schema, raw_toml))
}

async fn load_system_config_raw_toml(ctx: &WebContext) -> Result<String, String> {
    let store = ctx.config_store.read().await;
    store.config_toml_pretty().map_err(|err| err.to_string())
}

async fn get_published_spec(State(ctx): State<WebContext>) -> impl IntoResponse {
    let spec = ctx.published_store.read().await.spec.clone();
    Json(spec)
}

async fn save_published_spec(
    State(ctx): State<WebContext>,
    Json(spec): Json<PublishedDisplaySpec>,
) -> impl IntoResponse {
    {
        let mut store = ctx.published_store.write().await;
        if let Err(err) = store.replace_spec(spec.clone()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }
    }

    {
        let mut state = ctx.state.write().await;
        spec.apply_to_runtime_state(&mut state);
    }

    let (snapshot, preview) = emit_invalidate_status_preview(&ctx).await;

    (
        StatusCode::OK,
        Json(json!({"ok": true, "snapshot": snapshot, "preview": preview})),
    )
        .into_response()
}

async fn list_studio_pages(State(ctx): State<WebContext>) -> impl IntoResponse {
    let pages = ctx.page_store.read().await.list_summaries();
    Json(StudioPageListResponse { pages })
}

fn led_runtime_snapshot(state: &AppState) -> LedRuntimeResponse {
    LedRuntimeResponse {
        playing: state.led_runtime.playing,
        controller_mode_enabled: state.led_runtime.controller_mode.enabled,
        controller_mode: LedControllerModeResponse {
            mode: state.led_runtime.controller_mode.mode,
            speed: state.led_runtime.controller_mode.speed,
            color_index: state.led_runtime.controller_mode.color_index,
        },
    }
}

fn power_state_snapshot(state: &AppState) -> PowerStateResponse {
    PowerStateResponse {
        standby_active: state.power.standby_active,
        led_requested_on: state.power.led_requested_on,
        led_effective_on: state.power.led_effective_on,
        fan_requested_on: state.power.fan_requested_on,
        fan_effective_on: state.power.fan_effective_on,
        fan_auto_forced_by_temp: state.power.fan_auto_forced_by_temp,
        fan_explicit_off_temp_warning: state.power.fan_explicit_off_temp_warning,
        fan_last_error: state.power.fan_last_error.clone(),
        fan_auto_on_temp_c: state.power.fan_auto_on_temp_c,
    }
}

fn parse_cpu_temp_celsius(raw: &str) -> Option<f32> {
    let trimmed = raw.trim();
    let value_text = trimmed.strip_suffix('C').unwrap_or(trimmed).trim();
    value_text.parse::<f32>().ok()
}

fn refresh_power_runtime_derived_fields(state: &mut AppState) {
    let cpu_temp_c = parse_cpu_temp_celsius(&state.metrics.cpu_temp_text);
    let threshold = f32::from(state.power.fan_auto_on_temp_c);
    let over_threshold = cpu_temp_c.is_some_and(|temp| temp >= threshold);

    state.power.led_effective_on = state.power.led_requested_on && !state.power.standby_active;
    state.power.fan_auto_forced_by_temp = state.power.standby_active && over_threshold;
    state.power.fan_explicit_off_temp_warning =
        !state.power.standby_active && !state.power.fan_requested_on && over_threshold;
    state.power.fan_effective_on = if state.power.standby_active {
        state.power.fan_auto_forced_by_temp
    } else {
        state.power.fan_requested_on
    };
}

async fn build_led_lab_state(ctx: &WebContext) -> LedLabStateResponse {
    let (mut entries, mut studio_modes) = {
        let store = ctx.led_lab_store.read().await;
        (store.entries.clone(), store.studio_modes.clone())
    };
    studio_modes.sort_by(|a, b| a.id.cmp(&b.id));
    entries.sort_by(|a, b| {
        a.category
            .as_id()
            .cmp(b.category.as_id())
            .then_with(|| a.register.cmp(&b.register))
            .then_with(|| a.value.cmp(&b.value))
            .then_with(|| a.label.cmp(&b.label))
    });

    let categories = build_led_lab_category_coverage(&entries);
    let register_groups = build_led_lab_register_coverage(&entries);

    LedLabStateResponse {
        known_facts: known_facts(),
        studio_modes,
        entries,
        categories,
        register_groups,
        runner: ctx.led_lab_runner.snapshot().await,
        defaults: LedLabDefaults {
            bus: LED_LAB_BUS,
            led_device_address: LED_LAB_DEVICE_ADDRESS,
            oled_device_address: OLED_DEVICE_ADDRESS,
        },
    }
}

#[derive(Clone, Copy)]
struct RegisterGroupDef {
    id: &'static str,
    title: &'static str,
    category: LedLabCategory,
    command_class: LedLabCommandClass,
    register: u8,
    range_start: u8,
    range_end: u8,
    description: &'static str,
}

fn register_group_defs() -> Vec<RegisterGroupDef> {
    vec![
        RegisterGroupDef {
            id: "fan-08",
            title: "Fan Register 0x08",
            category: LedLabCategory::Fan,
            command_class: LedLabCommandClass::FanControl,
            register: 0x08,
            range_start: 0x00,
            range_end: 0x01,
            description: "Fan off/on control values.",
        },
        RegisterGroupDef {
            id: "effect-04",
            title: "Built-in Effect Register 0x04",
            category: LedLabCategory::BuiltinEffect,
            command_class: LedLabCommandClass::BuiltinEffect,
            register: 0x04,
            range_start: 0x00,
            range_end: 0x06,
            description: "Built-in effect selector table (0x00 is overloaded with direct mode arm).",
        },
        RegisterGroupDef {
            id: "speed-05",
            title: "Built-in Speed Register 0x05",
            category: LedLabCategory::BuiltinSpeed,
            command_class: LedLabCommandClass::BuiltinSpeed,
            register: 0x05,
            range_start: 0x01,
            range_end: 0x03,
            description: "Built-in speed selector values.",
        },
        RegisterGroupDef {
            id: "color-06",
            title: "Built-in Color Register 0x06",
            category: LedLabCategory::BuiltinColor,
            command_class: LedLabCommandClass::BuiltinColor,
            register: 0x06,
            range_start: 0x00,
            range_end: 0x06,
            description: "Built-in color preset selector values.",
        },
        RegisterGroupDef {
            id: "direct-enable-04",
            title: "Direct Mode Enable 0x04",
            category: LedLabCategory::DirectMode,
            command_class: LedLabCommandClass::DirectMode,
            register: 0x04,
            range_start: 0x00,
            range_end: 0x00,
            description: "Set 0x04=0x00 before direct LED index/RGB writes.",
        },
        RegisterGroupDef {
            id: "direct-index-00",
            title: "Direct Index Register 0x00",
            category: LedLabCategory::DirectChannel,
            command_class: LedLabCommandClass::DirectChannel,
            register: 0x00,
            range_start: 0x00,
            range_end: 0xFF,
            description: "Per-write LED index parameter when in direct mode.",
        },
        RegisterGroupDef {
            id: "direct-red-01",
            title: "Direct Red Register 0x01",
            category: LedLabCategory::DirectChannel,
            command_class: LedLabCommandClass::DirectChannel,
            register: 0x01,
            range_start: 0x00,
            range_end: 0xFF,
            description: "Per-write red channel parameter in direct mode.",
        },
        RegisterGroupDef {
            id: "direct-green-02",
            title: "Direct Green Register 0x02",
            category: LedLabCategory::DirectChannel,
            command_class: LedLabCommandClass::DirectChannel,
            register: 0x02,
            range_start: 0x00,
            range_end: 0xFF,
            description: "Per-write green channel parameter in direct mode.",
        },
        RegisterGroupDef {
            id: "direct-blue-03",
            title: "Direct Blue Register 0x03",
            category: LedLabCategory::DirectChannel,
            command_class: LedLabCommandClass::DirectChannel,
            register: 0x03,
            range_start: 0x00,
            range_end: 0xFF,
            description: "Per-write blue channel parameter in direct mode.",
        },
    ]
}

fn build_led_lab_category_coverage(entries: &[LedLabEntry]) -> Vec<LedLabCategoryCoverage> {
    let all_categories = [
        LedLabCategory::Fan,
        LedLabCategory::BuiltinEffect,
        LedLabCategory::BuiltinSpeed,
        LedLabCategory::BuiltinColor,
        LedLabCategory::DirectMode,
        LedLabCategory::DirectChannel,
        LedLabCategory::Unknown,
    ];

    let mut out = Vec::new();
    for category in all_categories {
        let category_entries: Vec<&LedLabEntry> = entries
            .iter()
            .filter(|entry| entry.category == category)
            .collect();
        if category_entries.is_empty() {
            continue;
        }

        let mut confirmed = 0usize;
        let mut likely = 0usize;
        let mut unknown = 0usize;
        let mut conflicting = 0usize;

        for entry in &category_entries {
            match entry.confidence {
                LedLabConfidence::Confirmed => confirmed += 1,
                LedLabConfidence::Likely => likely += 1,
                LedLabConfidence::Unknown => unknown += 1,
                LedLabConfidence::Conflicting => conflicting += 1,
            }
        }

        out.push(LedLabCategoryCoverage {
            id: category.as_id().to_string(),
            label: category.as_label().to_string(),
            total_entries: category_entries.len(),
            confirmed,
            likely,
            unknown,
            conflicting,
        });
    }

    out
}

fn build_led_lab_register_coverage(entries: &[LedLabEntry]) -> Vec<LedLabRegisterCoverage> {
    let defs = register_group_defs();
    let mut out = Vec::new();

    for def in defs {
        let mut known_slots = HashSet::new();
        for entry in entries {
            if entry.category != def.category || entry.register != def.register {
                continue;
            }
            if entry.value < def.range_start || entry.value > def.range_end {
                continue;
            }
            if entry.confidence != LedLabConfidence::Unknown {
                known_slots.insert(entry.value);
            }
        }

        let total_slots = usize::from(def.range_end - def.range_start) + 1;
        let known_slots_count = known_slots.len().min(total_slots);
        out.push(LedLabRegisterCoverage {
            id: def.id.to_string(),
            title: def.title.to_string(),
            category_id: def.category.as_id().to_string(),
            category_label: def.category.as_label().to_string(),
            command_class: def.command_class.as_label().to_string(),
            register: def.register,
            range_start: def.range_start,
            range_end: def.range_end,
            total_slots,
            known_slots: known_slots_count,
            unknown_slots: total_slots.saturating_sub(known_slots_count),
            description: def.description.to_string(),
        });
    }

    out
}

async fn get_led_runtime(State(ctx): State<WebContext>) -> impl IntoResponse {
    let state = ctx.state.read().await;
    Json(led_runtime_snapshot(&state))
}

async fn get_led_state(State(ctx): State<WebContext>) -> impl IntoResponse {
    let (runtime, led_output) = {
        let state = ctx.state.read().await;
        (led_runtime_snapshot(&state), state.led_output.clone())
    };

    Json(LedStateResponse {
        runtime,
        hardware_led_count: led_output.hardware_led_count.max(1),
        live_frame: led_output.frame,
        execution_mode: led_output.execution_mode.as_str().to_string(),
        frame_is_estimated: led_output.frame_is_estimated,
        offloaded: led_output.offloaded.map(|offloaded| LedOffloadedResponse {
            mode: offloaded.mode,
            speed: offloaded.speed,
            color_index: offloaded.color_index,
            source_effect_id: offloaded.source_effect_id,
        }),
        physically_synced: led_output.physically_synced,
        last_error: led_output.last_error,
    })
}

async fn play_led_runtime(State(ctx): State<WebContext>) -> impl IntoResponse {
    let snapshot = {
        let mut state = ctx.state.write().await;
        state.led_runtime.playing = true;
        led_runtime_snapshot(&state)
    };

    (
        StatusCode::OK,
        Json(json!({"ok": true, "runtime": snapshot})),
    )
        .into_response()
}

async fn pause_led_runtime(State(ctx): State<WebContext>) -> impl IntoResponse {
    let snapshot = {
        let mut state = ctx.state.write().await;
        state.led_runtime.playing = false;
        led_runtime_snapshot(&state)
    };

    (
        StatusCode::OK,
        Json(json!({"ok": true, "runtime": snapshot})),
    )
        .into_response()
}

async fn stop_led_runtime(State(ctx): State<WebContext>) -> impl IntoResponse {
    let snapshot = {
        let mut state = ctx.state.write().await;
        state.led_runtime.playing = false;
        led_runtime_snapshot(&state)
    };

    (
        StatusCode::OK,
        Json(json!({"ok": true, "runtime": snapshot})),
    )
        .into_response()
}

async fn set_led_controller_mode(
    State(ctx): State<WebContext>,
    Json(req): Json<SetLedControllerModeRequest>,
) -> impl IntoResponse {
    let do_mode_zero_init = req.enabled && req.mode == Some(0);
    let snapshot = {
        let mut state = ctx.state.write().await;
        let mut mode = state.led_runtime.controller_mode.mode;
        let mut speed = state.led_runtime.controller_mode.speed;
        let mut color_index = state.led_runtime.controller_mode.color_index;

        if let Some(value) = req.mode {
            mode = value;
        }
        if let Some(value) = req.speed {
            speed = value;
        }
        if let Some(value) = req.color_index {
            color_index = value;
        }

        if mode > 6 {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": "mode must be between 0 and 6"})),
            )
                .into_response();
        }
        if !(1..=3).contains(&speed) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": "speed must be between 1 and 3"})),
            )
                .into_response();
        }
        if color_index > 6 {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": "color_index must be between 0 and 6"})),
            )
                .into_response();
        }

        state.led_runtime.controller_mode.mode = mode;
        state.led_runtime.controller_mode.speed = speed;
        state.led_runtime.controller_mode.color_index = color_index;
        state.led_runtime.controller_mode.enabled = req.enabled;
        if req.enabled {
            state.led_runtime.playing = true;
        }

        led_runtime_snapshot(&state)
    };

    if do_mode_zero_init {
        let led_cfg = {
            let cfg = ctx.config_store.read().await;
            cfg.config.led.clone()
        };
        let mut strip = match YahboomLedStrip::new(
            &led_cfg.i2c_path,
            led_cfg.address,
            led_cfg.hardware_led_count,
        ) {
            Ok(strip) => strip,
            Err(err) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"ok": false, "error": err.to_string()})),
                )
                    .into_response();
            }
        };
        if let Err(err) = strip.clear() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }
    }

    (
        StatusCode::OK,
        Json(json!({"ok": true, "runtime": snapshot})),
    )
        .into_response()
}

fn parse_hex_color(value: &str) -> std::result::Result<LedColor, String> {
    let text = value.trim().trim_start_matches('#');
    if text.len() != 6 || !text.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("hex must be a 6-digit RGB value (e.g. FFAA00)".to_string());
    }

    let r = u8::from_str_radix(&text[0..2], 16).map_err(|_| "invalid red hex byte".to_string())?;
    let g =
        u8::from_str_radix(&text[2..4], 16).map_err(|_| "invalid green hex byte".to_string())?;
    let b = u8::from_str_radix(&text[4..6], 16).map_err(|_| "invalid blue hex byte".to_string())?;
    Ok(LedColor::rgb(r, g, b))
}

async fn set_led_direct_pixel(
    State(ctx): State<WebContext>,
    Json(req): Json<LedDirectPixelRequest>,
) -> impl IntoResponse {
    if req.index > 13 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": "index must be between 0 and 13"})),
        )
            .into_response();
    }

    let color = match parse_hex_color(&req.hex) {
        Ok(color) => color,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": error})),
            )
                .into_response();
        }
    };

    let led_cfg = {
        let cfg = ctx.config_store.read().await;
        cfg.config.led.clone()
    };

    let mut strip = match YahboomLedStrip::new(
        &led_cfg.i2c_path,
        led_cfg.address,
        led_cfg.hardware_led_count,
    ) {
        Ok(strip) => strip,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }
    };

    if let Err(err) = strip.write_direct_pixel(req.index, color) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": err.to_string()})),
        )
            .into_response();
    }

    {
        let mut state = ctx.state.write().await;
        state.led_runtime.controller_mode.enabled = true;
        state.led_runtime.controller_mode.mode = 0;
        state.led_runtime.playing = true;
        if (req.index as usize) < state.led_output.frame.len() {
            state.led_output.frame[req.index as usize] = color;
            state.led_output.physically_synced = true;
            state.led_output.last_error = None;
        }
    }

    (
        StatusCode::OK,
        Json(json!({"ok": true, "runtime": {
            "mode": 0,
            "index": req.index,
            "hex": req.hex.trim().trim_start_matches('#').to_uppercase()
        }})),
    )
        .into_response()
}

async fn get_power_state(State(ctx): State<WebContext>) -> impl IntoResponse {
    let power = {
        let mut state = ctx.state.write().await;
        refresh_power_runtime_derived_fields(&mut state);
        power_state_snapshot(&state)
    };
    Json(power)
}

async fn set_fan_requested_state(
    Query(params): Query<ToggleParams>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    let (power, runtime) = {
        let mut state = ctx.state.write().await;
        state.power.fan_requested_on = params.enabled;
        refresh_power_runtime_derived_fields(&mut state);
        (power_state_snapshot(&state), led_runtime_snapshot(&state))
    };

    (
        StatusCode::OK,
        Json(json!({"ok": true, "power": power, "runtime": runtime})),
    )
        .into_response()
}

async fn set_led_requested_state(
    Query(params): Query<ToggleParams>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    let (power, runtime) = {
        let mut state = ctx.state.write().await;
        state.power.led_requested_on = params.enabled;
        refresh_power_runtime_derived_fields(&mut state);
        (power_state_snapshot(&state), led_runtime_snapshot(&state))
    };

    (
        StatusCode::OK,
        Json(json!({"ok": true, "power": power, "runtime": runtime})),
    )
        .into_response()
}

async fn set_standby_state(
    Query(params): Query<ToggleParams>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    let (power, runtime) = {
        let mut state = ctx.state.write().await;
        state.power.standby_active = params.enabled;
        refresh_power_runtime_derived_fields(&mut state);
        (power_state_snapshot(&state), led_runtime_snapshot(&state))
    };

    (
        StatusCode::OK,
        Json(json!({"ok": true, "power": power, "runtime": runtime})),
    )
        .into_response()
}

async fn set_fan_auto_threshold(
    Query(params): Query<FanAutoThresholdParams>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    if !(30..=110).contains(&params.c) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": "threshold must be between 30C and 110C"})),
        )
            .into_response();
    }

    {
        let mut store = ctx.config_store.write().await;
        let mut next = store.config.clone();
        next.led.fan_auto_on_temp_c = params.c;
        if let Err(err) = store.replace_config(next) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }
    }

    let power = {
        let mut state = ctx.state.write().await;
        state.power.fan_auto_on_temp_c = params.c;
        refresh_power_runtime_derived_fields(&mut state);
        power_state_snapshot(&state)
    };

    (StatusCode::OK, Json(json!({"ok": true, "power": power}))).into_response()
}

fn validate_led_lab_target(bus: u8, device_address: u8) -> Result<(), String> {
    if bus != LED_LAB_BUS {
        return Err(format!(
            "LED lab bus is fixed to {} (requested {})",
            LED_LAB_BUS, bus
        ));
    }
    if device_address != LED_LAB_DEVICE_ADDRESS {
        return Err(format!(
            "LED lab device address is fixed to 0x{:02x} (requested 0x{:02x})",
            LED_LAB_DEVICE_ADDRESS, device_address
        ));
    }
    Ok(())
}

async fn ensure_led_runtime_paused_for_lab(ctx: &WebContext) -> bool {
    let mut state = ctx.state.write().await;
    if state.led_runtime.playing {
        state.led_runtime.playing = false;
        return true;
    }
    false
}

async fn get_led_lab_state(State(ctx): State<WebContext>) -> impl IntoResponse {
    Json(build_led_lab_state(&ctx).await)
}

async fn run_led_lab_steps(
    State(ctx): State<WebContext>,
    Json(req): Json<LedLabRunRequest>,
) -> impl IntoResponse {
    if let Err(err) = validate_led_lab_target(req.bus, req.device_address) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": err})),
        )
            .into_response();
    }

    let runtime_auto_paused = ensure_led_runtime_paused_for_lab(&ctx).await;

    let result = ctx
        .led_lab_runner
        .execute_steps(&req.label, req.steps)
        .await;
    (
        StatusCode::OK,
        Json(json!({
            "ok": result.success,
            "result": result,
            "runtime_auto_paused": runtime_auto_paused,
            "state": build_led_lab_state(&ctx).await
        })),
    )
        .into_response()
}

async fn run_led_lab_direct_single_color(
    State(ctx): State<WebContext>,
    Json(req): Json<LedLabDirectSingleColorRequest>,
) -> impl IntoResponse {
    let runtime_auto_paused = ensure_led_runtime_paused_for_lab(&ctx).await;
    let steps = direct_single_color_steps(req.index, req.red, req.green, req.blue, req.delay_ms);
    let result = ctx
        .led_lab_runner
        .execute_steps("direct_single_color", steps)
        .await;

    (
        StatusCode::OK,
        Json(json!({
            "ok": result.success,
            "result": result,
            "runtime_auto_paused": runtime_auto_paused,
            "state": build_led_lab_state(&ctx).await
        })),
    )
        .into_response()
}

async fn run_led_lab_scan(
    State(ctx): State<WebContext>,
    Json(req): Json<LedLabScanRequest>,
) -> impl IntoResponse {
    if let Err(err) = validate_led_lab_target(req.bus, req.device_address) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": err})),
        )
            .into_response();
    }

    let runtime_auto_paused = ensure_led_runtime_paused_for_lab(&ctx).await;

    if let Err(err) = validate_scan_bounds(
        req.register_start,
        req.register_end,
        req.value_start,
        req.value_end,
        req.allow_larger,
    ) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": err.to_string()})),
        )
            .into_response();
    }

    let steps = build_scan_steps(
        req.register_start,
        req.register_end,
        req.value_start,
        req.value_end,
        req.delay_ms,
    );
    let label = req
        .label
        .unwrap_or_else(|| "cautious register/value scan".to_string());
    let result = ctx.led_lab_runner.execute_steps(&label, steps).await;

    (
        StatusCode::OK,
        Json(json!({
            "ok": result.success,
            "result": result,
            "runtime_auto_paused": runtime_auto_paused,
            "state": build_led_lab_state(&ctx).await
        })),
    )
        .into_response()
}

async fn save_led_lab_entry(
    State(_ctx): State<WebContext>,
    Json(_draft): Json<LedLabEntryDraft>,
) -> impl IntoResponse {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "ok": false,
            "error": "LED lab commands are immutable in-panel. Edit config/led-lab.json directly."
        })),
    )
        .into_response()
}

async fn run_led_lab_entry(
    Path(id): Path<String>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    let runtime_auto_paused = ensure_led_runtime_paused_for_lab(&ctx).await;

    let entry = {
        let store = ctx.led_lab_store.read().await;
        match store.get_entry(&id) {
            Some(entry) => entry.clone(),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"ok": false, "error": "LED lab entry not found"})),
                )
                    .into_response();
            }
        }
    };

    let result = ctx
        .led_lab_runner
        .execute_steps(&entry.label, entry.steps.clone())
        .await;

    (
        StatusCode::OK,
        Json(json!({
            "ok": result.success,
            "entry": entry,
            "result": result,
            "runtime_auto_paused": runtime_auto_paused,
            "state": build_led_lab_state(&ctx).await
        })),
    )
        .into_response()
}

async fn delete_led_lab_entry(
    Path(_id): Path<String>,
    State(_ctx): State<WebContext>,
) -> impl IntoResponse {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "ok": false,
            "error": "LED lab commands are immutable in-panel. Edit config/led-lab.json directly."
        })),
    )
        .into_response()
}

async fn abort_led_lab_run(State(ctx): State<WebContext>) -> impl IntoResponse {
    let running = ctx.led_lab_runner.request_abort().await;
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "running": running,
            "state": build_led_lab_state(&ctx).await
        })),
    )
        .into_response()
}

async fn get_studio_catalog(State(ctx): State<WebContext>) -> impl IntoResponse {
    let pages = build_studio_catalog_entries(&ctx).await;
    Json(StudioCatalogResponse { pages })
}

async fn get_studio_catalog_page(
    Path(key): Path<String>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    let Some(parsed_page_id) = parse_catalog_key(&key) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": "unknown page key"})),
        )
            .into_response();
    };
    let (page, definition) = {
        let store = ctx.page_store.read().await;
        let resolved_page_id = if store.get(&parsed_page_id).is_some() {
            parsed_page_id.clone()
        } else {
            canonical_page_id(&parsed_page_id)
        };

        let Some(page) = catalog_entry_for_page_id(&store, &resolved_page_id) else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"ok": false, "error": "page not found"})),
            )
                .into_response();
        };
        let definition = store.get(&resolved_page_id).cloned();
        (page, definition)
    };

    (
        StatusCode::OK,
        Json(StudioPageDetailResponse { page, definition }),
    )
        .into_response()
}

async fn get_studio_page(
    Path(id): Path<String>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    let page = ctx.page_store.read().await.get(&id).cloned();
    match page {
        Some(page) => (StatusCode::OK, Json(page)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"ok": false, "error": "page not found"})),
        )
            .into_response(),
    }
}

async fn export_studio_page(
    Path(id): Path<String>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    let raw = {
        let store = ctx.page_store.read().await;
        match store.export_page_json_pretty(&id) {
            Ok(raw) => raw,
            Err(err) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"ok": false, "error": err.to_string()})),
                )
                    .into_response();
            }
        }
    };

    (
        StatusCode::OK,
        [("content-type", "application/json; charset=utf-8")],
        raw,
    )
        .into_response()
}

async fn import_studio_page(
    Query(query): Query<ImportPageQuery>,
    State(ctx): State<WebContext>,
    Json(raw_page): Json<Value>,
) -> impl IntoResponse {
    let policy = match parse_import_conflict_policy(query.conflict.as_deref()) {
        Ok(policy) => policy,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err})),
            )
                .into_response();
        }
    };

    let (width, height) = {
        let cfg = ctx.config_store.read().await;
        (cfg.config.display.width, cfg.config.display.height)
    };

    let page = match parse_page_definition_from_json_value(raw_page) {
        Ok(page) => page,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }
    };

    let imported_id = {
        let mut store = ctx.page_store.write().await;
        match store.import_page(page, policy, width, height) {
            Ok(id) => id,
            Err(err) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"ok": false, "error": err.to_string()})),
                )
                    .into_response();
            }
        }
    };

    let _ = ctx.event_tx.send(ServerEvent::Invalidate);

    (StatusCode::OK, Json(json!({"ok": true, "id": imported_id}))).into_response()
}

async fn create_studio_page(
    Query(query): Query<CreatePageQuery>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    let (width, height) = {
        let cfg = ctx.config_store.read().await;
        (cfg.config.display.width, cfg.config.display.height)
    };

    let created = {
        let mut store = ctx.page_store.write().await;
        match store.create_blank_page(&query.name, width, height) {
            Ok(id) => id,
            Err(err) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"ok": false, "error": err.to_string()})),
                )
                    .into_response();
            }
        }
    };

    let _ = ctx.event_tx.send(ServerEvent::Invalidate);

    (StatusCode::OK, Json(json!({"ok": true, "id": created}))).into_response()
}

async fn rename_studio_page(
    Path(id): Path<String>,
    Query(query): Query<RenamePageQuery>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    {
        let mut store = ctx.page_store.write().await;
        if let Err(err) = store.rename_page(&id, &query.name) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }
    }

    let _ = ctx.event_tx.send(ServerEvent::Invalidate);
    (StatusCode::OK, Json(json!({"ok": true}))).into_response()
}

async fn rekey_studio_page(
    Path(id): Path<String>,
    State(ctx): State<WebContext>,
    Json(req): Json<RekeyPageRequest>,
) -> impl IntoResponse {
    let new_id = req.new_id.trim().to_string();
    if new_id.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": "new page id cannot be empty"})),
        )
            .into_response();
    }

    {
        let mut store = ctx.page_store.write().await;
        if let Err(err) = store.rekey_page(&id, &new_id) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }
    }

    {
        let mut published = ctx.published_store.write().await;
        let mut changed = false;
        for step in &mut published.spec.boot_sequence {
            if step.page_id == id {
                step.page_id = new_id.clone();
                changed = true;
            }
        }
        for page_id in &mut published.spec.rotation_queue {
            if *page_id == id {
                *page_id = new_id.clone();
                changed = true;
            }
        }
        if changed {
            let _ = published.save();
        }
    }

    {
        let mut state = ctx.state.write().await;
        for page in &mut state.rotation_queue {
            if page.page_id() == id {
                *page = RuntimePage::from_id(new_id.clone());
            }
        }
        if state.active_page.page_id() == id {
            state.active_page = RuntimePage::from_id(new_id.clone());
        }
    }

    let (snapshot, preview) = emit_invalidate_status_preview(&ctx).await;
    (
        StatusCode::OK,
        Json(json!({"ok": true, "id": new_id, "snapshot": snapshot, "preview": preview})),
    )
        .into_response()
}

async fn delete_studio_page(
    Path(id): Path<String>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    {
        let mut store = ctx.page_store.write().await;
        if let Err(err) = store.delete_page(&id) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }
    }
    let fallback_page_id = {
        let store = ctx.page_store.read().await;
        store.pages.first().map(|page| page.id.clone())
    };

    {
        let mut published = ctx.published_store.write().await;
        published.spec.boot_sequence.retain(|s| s.page_id != id);
        published
            .spec
            .rotation_queue
            .retain(|page_id| page_id != &id);
        if published.spec.rotation_queue.is_empty() {
            if let Some(fallback) = fallback_page_id.clone() {
                published.spec.rotation_queue = vec![fallback];
            }
        }
        let _ = published.save();
    }

    {
        let mut state = ctx.state.write().await;
        state.rotation_queue.retain(|page| page.page_id() != id);
        if state.rotation_queue.is_empty() {
            if let Some(fallback) = fallback_page_id.clone() {
                state.rotation_queue = vec![RuntimePage::from_id(fallback)];
                state.rotation_index = 0;
            }
        } else if state.rotation_index >= state.rotation_queue.len() {
            state.rotation_index = 0;
        }
        if state.active_page.page_id() == id {
            if let Some(fallback) = fallback_page_id {
                state.active_page = RuntimePage::from_id(fallback);
            }
        }
    }

    let (snapshot, preview) = emit_invalidate_status_preview(&ctx).await;

    (
        StatusCode::OK,
        Json(json!({"ok": true, "snapshot": snapshot, "preview": preview})),
    )
        .into_response()
}

async fn replace_studio_page(
    Path(id): Path<String>,
    State(ctx): State<WebContext>,
    Json(raw_page): Json<Value>,
) -> impl IntoResponse {
    let page = match parse_page_definition_from_json_value(raw_page) {
        Ok(page) => page,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }
    };

    {
        let mut store = ctx.page_store.write().await;
        if let Err(err) = store.replace_page(&id, page) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }
    }

    let _ = ctx.event_tx.send(ServerEvent::Invalidate);
    (StatusCode::OK, Json(json!({"ok": true}))).into_response()
}

async fn apply_studio_page_to_live(
    Path(id): Path<String>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    let exists = ctx.page_store.read().await.get(&id).is_some();
    if !exists {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"ok": false, "error": "page not found"})),
        )
            .into_response();
    }

    {
        let mut state = ctx.state.write().await;
        state.active_page = RuntimePage::from_id(id.clone());
        state.display_mode = DisplayMode::Manual;
    }
    let (snapshot, preview) = emit_invalidate_status_preview(&ctx).await;

    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "active_page": id,
            "snapshot": snapshot,
            "preview": preview
        })),
    )
        .into_response()
}

async fn add_static_text_element(
    Path(id): Path<String>,
    State(ctx): State<WebContext>,
    Json(req): Json<AddStaticTextRequest>,
) -> impl IntoResponse {
    let size = req.size.unwrap_or_default();
    let element = PageElement::StaticText {
        x: req.x,
        y: req.y,
        text: req.text,
        size: size.clone(),
        text_height_px: req.text_height_px.unwrap_or_else(|| match size {
            TextSize::Small => 6,
            TextSize::Large => 10,
        }),
        color: req.color.unwrap_or_default(),
        name: normalized_element_name(req.name),
    };

    mutate_page_store_element(ctx, id, element).await
}

async fn add_dynamic_text_element(
    Path(id): Path<String>,
    State(ctx): State<WebContext>,
    Json(req): Json<AddDynamicTextRequest>,
) -> impl IntoResponse {
    let size = req.size.unwrap_or_default();
    let element = PageElement::DynamicText {
        x: req.x,
        y: req.y,
        source: req.source,
        prefix: req.prefix.unwrap_or_default(),
        max_chars: req.max_chars.unwrap_or(20),
        size: size.clone(),
        text_height_px: req.text_height_px.unwrap_or_else(|| match size {
            TextSize::Small => 6,
            TextSize::Large => 10,
        }),
        color: req.color.unwrap_or_default(),
        name: normalized_element_name(req.name),
    };

    mutate_page_store_element(ctx, id, element).await
}

async fn add_rect_element(
    Path(id): Path<String>,
    State(ctx): State<WebContext>,
    Json(req): Json<AddRectRequest>,
) -> impl IntoResponse {
    let element = PageElement::Rect {
        x: req.x,
        y: req.y,
        w: req.w,
        h: req.h,
        fill: req.fill,
        stroke: req.stroke,
        name: normalized_element_name(req.name),
        filled: req.filled,
    };

    mutate_page_store_element(ctx, id, element).await
}

async fn add_image_element(
    Path(id): Path<String>,
    State(ctx): State<WebContext>,
    Json(req): Json<AddImageRequest>,
) -> impl IntoResponse {
    let element = PageElement::Image {
        x: req.x,
        y: req.y,
        w: req.w.max(1),
        h: req.h.max(1),
        source: req.source.trim().to_string(),
        mask_mode: req.mask_mode.unwrap_or_default(),
        threshold: req.threshold.unwrap_or(128),
        foreground: req.foreground.unwrap_or_default(),
        background: req.background,
        name: normalized_element_name(req.name),
    };

    mutate_page_store_element(ctx, id, element).await
}

async fn add_line_element(
    Path(id): Path<String>,
    State(ctx): State<WebContext>,
    Json(req): Json<AddLineRequest>,
) -> impl IntoResponse {
    let element = PageElement::Line {
        x1: req.x1,
        y1: req.y1,
        x2: req.x2,
        y2: req.y2,
        color: req.color.unwrap_or_default(),
        name: normalized_element_name(req.name),
    };

    mutate_page_store_element(ctx, id, element).await
}

async fn delete_studio_element(
    Path((id, index)): Path<(String, usize)>,
    State(ctx): State<WebContext>,
) -> impl IntoResponse {
    {
        let mut store = ctx.page_store.write().await;
        if let Err(err) = store.delete_element(&id, index) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }
    }

    let _ = ctx.event_tx.send(ServerEvent::Invalidate);
    (StatusCode::OK, Json(json!({"ok": true}))).into_response()
}

async fn mutate_page_store_element(
    ctx: WebContext,
    id: String,
    element: PageElement,
) -> impl IntoResponse {
    {
        let mut store = ctx.page_store.write().await;
        if let Err(err) = store.push_element(&id, element) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": err.to_string()})),
            )
                .into_response();
        }
    }

    let _ = ctx.event_tx.send(ServerEvent::Invalidate);
    (StatusCode::OK, Json(json!({"ok": true}))).into_response()
}

async fn build_studio_catalog_entries(ctx: &WebContext) -> Vec<CatalogPageEntry> {
    let store = ctx.page_store.read().await;
    build_catalog(&store)
}

async fn emit_status_preview(ctx: &WebContext) -> (UiSnapshot, PreviewSnapshot) {
    let preview = build_live_preview_snapshot_from_ctx(ctx).await;
    let snapshot = {
        let state = ctx.state.read().await;
        UiSnapshot::from_state(&state)
    };

    let _ = ctx.event_tx.send(ServerEvent::Status(snapshot.clone()));
    let _ = ctx.event_tx.send(ServerEvent::Preview(preview.clone()));

    (snapshot, preview)
}

async fn emit_invalidate_status_preview(ctx: &WebContext) -> (UiSnapshot, PreviewSnapshot) {
    let _ = ctx.event_tx.send(ServerEvent::Invalidate);
    emit_status_preview(ctx).await
}

async fn build_live_preview_snapshot_from_ctx(ctx: &WebContext) -> PreviewSnapshot {
    let state = ctx.state.read().await.clone();
    let config = ctx.config_store.read().await.config.clone();
    build_live_preview_snapshot(ctx, &state, &config).await
}

async fn build_live_preview_snapshot(
    ctx: &WebContext,
    state: &AppState,
    config: &AppConfig,
) -> PreviewSnapshot {
    let frame = render_runtime_page_frame(ctx, config, state, &state.active_page).await;

    PreviewSnapshot {
        active_page: state.active_page.label(),
        frame,
    }
}

async fn build_studio_preview_snapshot(ctx: &WebContext, page_key: &str) -> PreviewSnapshot {
    let (state, config) = {
        let state = ctx.state.read().await.clone();
        let config = ctx.config_store.read().await.config.clone();
        (state, config)
    };

    let frame = match runtime_page_from_catalog_key(page_key) {
        Some(runtime_page) => render_runtime_page_frame(ctx, &config, &state, &runtime_page).await,
        None => blank_frame(
            config.display.width as usize,
            config.display.height as usize,
        ),
    };

    PreviewSnapshot {
        active_page: page_key.to_string(),
        frame,
    }
}

fn runtime_page_from_catalog_key(page_key: &str) -> Option<RuntimePage> {
    let page_id = parse_catalog_key(page_key)?;
    Some(RuntimePage::from_id(page_id))
}

async fn render_runtime_page_frame(
    ctx: &WebContext,
    config: &AppConfig,
    state: &AppState,
    runtime_page: &RuntimePage,
) -> PreviewFrame {
    let maybe_page = {
        let store = ctx.page_store.read().await;
        store.get(runtime_page.page_id()).cloned().or_else(|| {
            let canonical = canonical_page_id(runtime_page.page_id());
            if canonical == runtime_page.page_id() {
                None
            } else {
                store.get(&canonical).cloned()
            }
        })
    };
    match maybe_page {
        Some(page) => {
            let dynamic_context = dynamic_render_context_from_state_config(state, config);
            render_page_definition_to_frame(
                config.display.width as usize,
                config.display.height as usize,
                &page,
                &state.metrics,
                &dynamic_context,
            )
        }
        None => blank_frame(
            config.display.width as usize,
            config.display.height as usize,
        ),
    }
}

fn blank_frame(width: usize, height: usize) -> PreviewFrame {
    PreviewFrame::new(width, height)
}

async fn resolve_existing_page_id(ctx: &WebContext, requested: &str) -> Option<String> {
    let store = ctx.page_store.read().await;

    if store.get(requested).is_some() {
        return Some(requested.to_string());
    }

    let canonical = canonical_page_id(requested);
    if store.get(&canonical).is_some() {
        return Some(canonical);
    }

    None
}

fn parse_import_conflict_policy(
    value: Option<&str>,
) -> std::result::Result<ImportConflictPolicy, &'static str> {
    match value.unwrap_or("duplicate") {
        "duplicate" => Ok(ImportConflictPolicy::Duplicate),
        "replace" => Ok(ImportConflictPolicy::Replace),
        "error" => Ok(ImportConflictPolicy::Error),
        _ => Err("invalid conflict policy; expected one of: duplicate, replace, error"),
    }
}
