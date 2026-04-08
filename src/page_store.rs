use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::RwLock;

pub type SharedPageStore = Arc<RwLock<PageStore>>;

pub const PAGE_ID_BOOT: &str = "solstice-panel-core-1.0.0-boot";
pub const PAGE_ID_LIVE_INFO: &str = "solstice-panel-core-1.0.0-live-info";
pub const PAGE_ID_DIAGNOSTICS: &str = "solstice-panel-core-1.0.0-diagnostics";
pub const LEGACY_PAGE_ID_BOOT: &str = "boot";
pub const LEGACY_PAGE_ID_LIVE_INFO: &str = "live_info";
pub const LEGACY_PAGE_ID_DIAGNOSTICS: &str = "diagnostics";
pub const PAGE_DEFINITION_SCHEMA_VERSION: u32 = 1;
pub const PAGE_VERSION_DEFAULT: &str = "1.0.0";
pub const TEXT_HEIGHT_MIN_PX: u32 = 5;
const LEGACY_BOOT_MAIN_TEXT: &str = "SOLSTICE";
const LEGACY_BOOT_SUB_TEXT: &str = "Rust panel starting...";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageMeta {
    #[serde(default = "default_page_schema_version")]
    pub schema_version: u32,
    #[serde(default = "default_page_version")]
    pub version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub bundle_name: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Default for PageMeta {
    fn default() -> Self {
        Self {
            schema_version: default_page_schema_version(),
            version: default_page_version(),
            authors: Vec::new(),
            description: None,
            bundle_name: None,
            license: None,
            source_url: None,
            tags: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportConflictPolicy {
    Error,
    Replace,
    Duplicate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageDefinition {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub meta: PageMeta,
    pub elements: Vec<PageElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextSize {
    Small,
    Large,
}

impl Default for TextSize {
    fn default() -> Self {
        Self::Small
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MonoColor {
    #[default]
    On,
    Off,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImageMaskMode {
    #[default]
    Alpha,
    AlphaInverted,
    LumaLight,
    LumaDark,
}

fn default_image_threshold() -> u8 {
    128
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DynamicSource {
    #[serde(rename = "sys:hostname", alias = "hostname")]
    Hostname,
    #[serde(rename = "sys:ip_addr", alias = "ip_addr")]
    IpAddr,
    #[serde(rename = "sys:local_time_hm", alias = "local_time_hm")]
    LocalTimeHm,
    #[serde(rename = "sys:local_time_hms", alias = "local_time_hms")]
    LocalTimeHms,
    #[serde(rename = "sys:local_date_ymd", alias = "local_date_ymd")]
    LocalDateYmd,
    #[serde(rename = "sys:local_date_dmy", alias = "local_date_dmy")]
    LocalDateDmy,
    #[serde(rename = "sys:local_date_mdy", alias = "local_date_mdy")]
    LocalDateMdy,
    #[serde(rename = "sys:local_datetime_iso", alias = "local_datetime_iso")]
    LocalDateTimeIso,
    #[serde(
        rename = "sys:local_datetime_compact",
        alias = "local_datetime_compact"
    )]
    LocalDateTimeCompact,
    #[serde(
        rename = "sys:local_datetime_rfc2822",
        alias = "local_datetime_rfc2822"
    )]
    LocalDateTimeRfc2822,
    #[serde(
        rename = "sys:local_datetime_rfc3339",
        alias = "local_datetime_rfc3339"
    )]
    LocalDateTimeRfc3339,
    #[serde(rename = "sys:uptime_text", alias = "uptime_text")]
    UptimeText,
    #[serde(rename = "sys:ram_percent_text", alias = "ram_percent_text")]
    RamPercentText,
    #[serde(rename = "sys:cpu_temp_text", alias = "cpu_temp_text")]
    CpuTempText,
    #[serde(rename = "sys:cpu_usage_percent", alias = "cpu_usage_percent")]
    CpuUsagePercentText,
    #[serde(rename = "sys:load_avg_1", alias = "load_avg_1")]
    LoadAvg1,
    #[serde(rename = "sys:load_avg_5", alias = "load_avg_5")]
    LoadAvg5,
    #[serde(rename = "sys:load_avg_15", alias = "load_avg_15")]
    LoadAvg15,
    #[serde(rename = "sys:load_avg_text", alias = "load_avg_text")]
    LoadAvgText,
    #[serde(rename = "sys:mem_total_mib", alias = "mem_total_mib")]
    MemTotalMibText,
    #[serde(rename = "sys:mem_used_mib", alias = "mem_used_mib")]
    MemUsedMibText,
    #[serde(rename = "sys:mem_available_mib", alias = "mem_available_mib")]
    MemAvailableMibText,
    #[serde(rename = "sys:mem_free_mib", alias = "mem_free_mib")]
    MemFreeMibText,
    #[serde(rename = "sys:swap_total_mib", alias = "swap_total_mib")]
    SwapTotalMibText,
    #[serde(rename = "sys:swap_used_mib", alias = "swap_used_mib")]
    SwapUsedMibText,
    #[serde(rename = "sys:swap_free_mib", alias = "swap_free_mib")]
    SwapFreeMibText,
    #[serde(
        rename = "sys:swap_used_percent_text",
        alias = "swap_used_percent_text"
    )]
    SwapUsedPercentText,
    #[serde(rename = "sys:procs_running", alias = "procs_running")]
    ProcsRunning,
    #[serde(rename = "sys:procs_blocked", alias = "procs_blocked")]
    ProcsBlocked,
    #[serde(rename = "sys:cpu_cores", alias = "cpu_cores")]
    CpuCores,
    #[serde(rename = "sys:os_pretty_name", alias = "os_pretty_name")]
    OsPrettyName,
    #[serde(rename = "sys:kernel_release", alias = "kernel_release")]
    KernelRelease,
    #[serde(rename = "display:active_page", alias = "active_page")]
    ActivePage,
    #[serde(rename = "display:active_page_id", alias = "active_page_id")]
    ActivePageId,
    #[serde(rename = "display:mode", alias = "display_mode")]
    DisplayMode,
    #[serde(rename = "display:rotation_active", alias = "rotation_active")]
    RotationActive,
    #[serde(
        rename = "display:rotation_interval_ms",
        alias = "rotation_interval_ms"
    )]
    RotationIntervalMs,
    #[serde(
        rename = "display:rotation_interval_seconds",
        alias = "rotation_interval_seconds"
    )]
    RotationIntervalSeconds,
    #[serde(rename = "display:rotation_queue_len", alias = "rotation_queue_len")]
    RotationQueueLen,
    #[serde(
        rename = "display:rotation_queue_empty",
        alias = "rotation_queue_empty"
    )]
    RotationQueueEmpty,
    #[serde(rename = "display:rotation_index", alias = "rotation_index")]
    RotationIndex,
    #[serde(rename = "display:rotation_next_index", alias = "rotation_next_index")]
    RotationNextIndex,
    #[serde(rename = "display:rotation_position", alias = "rotation_position")]
    RotationPosition,
    #[serde(rename = "display:width", alias = "display_width")]
    DisplayWidth,
    #[serde(rename = "display:height", alias = "display_height")]
    DisplayHeight,
    #[serde(rename = "config:refresh_ms", alias = "refresh_ms")]
    RefreshMs,
    #[serde(rename = "config:i2c_address", alias = "i2c_address")]
    I2cAddress,
    #[serde(rename = "config:i2c_address_hex", alias = "i2c_address_hex")]
    I2cAddressHex,
    #[serde(rename = "config:web_bind", alias = "web_bind")]
    WebBind,
    #[serde(rename = "display:page_id", alias = "page_id")]
    PageId,
    #[serde(rename = "display:page_name", alias = "page_name")]
    PageName,
    #[serde(rename = "display:page_version", alias = "page_version")]
    PageVersion,
    #[serde(rename = "display:page_authors", alias = "page_authors")]
    PageAuthors,
    #[serde(rename = "display:page_author_count", alias = "page_author_count")]
    PageAuthorCount,
    #[serde(rename = "display:page_bundle", alias = "page_bundle")]
    PageBundle,
    #[serde(rename = "display:page_license", alias = "page_license")]
    PageLicense,
    #[serde(rename = "display:page_source_url", alias = "page_source_url")]
    PageSourceUrl,
    #[serde(rename = "display:page_tags", alias = "page_tags")]
    PageTags,
    #[serde(rename = "display:page_tag_count", alias = "page_tag_count")]
    PageTagCount,
    #[serde(rename = "display:page_description", alias = "page_description")]
    PageDescription,
    #[serde(rename = "display:page_element_count", alias = "page_element_count")]
    PageElementCount,
    #[serde(rename = "display:page_width", alias = "page_width")]
    PageWidth,
    #[serde(rename = "display:page_height", alias = "page_height")]
    PageHeight,
    #[serde(rename = "sys:unix_epoch_seconds", alias = "unix_epoch_seconds")]
    UnixEpochSeconds,
    #[serde(rename = "sys:unix_epoch_millis", alias = "unix_epoch_millis")]
    UnixEpochMillis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PageElement {
    StaticText {
        x: i32,
        y: i32,
        text: String,
        #[serde(default)]
        size: TextSize,
        #[serde(default)]
        text_height_px: u32,
        #[serde(default)]
        color: MonoColor,
        #[serde(default)]
        name: Option<String>,
    },
    DynamicText {
        x: i32,
        y: i32,
        source: DynamicSource,
        #[serde(default)]
        prefix: String,
        #[serde(default)]
        max_chars: usize,
        #[serde(default)]
        size: TextSize,
        #[serde(default)]
        text_height_px: u32,
        #[serde(default)]
        color: MonoColor,
        #[serde(default)]
        name: Option<String>,
    },
    Image {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        source: String,
        #[serde(default)]
        mask_mode: ImageMaskMode,
        #[serde(default = "default_image_threshold")]
        threshold: u8,
        #[serde(default)]
        foreground: MonoColor,
        #[serde(default)]
        background: Option<MonoColor>,
        #[serde(default)]
        name: Option<String>,
    },
    Rect {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        #[serde(default)]
        fill: Option<MonoColor>,
        #[serde(default)]
        stroke: Option<MonoColor>,
        #[serde(default)]
        name: Option<String>,
        // Legacy compatibility fallback: true => filled on, false => stroke on.
        #[serde(default)]
        filled: bool,
    },
    Line {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        #[serde(default)]
        color: MonoColor,
        #[serde(default)]
        name: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct PageSummary {
    pub id: String,
    pub name: String,
    pub element_count: usize,
}

#[derive(Debug, Clone)]
pub struct PageStore {
    pub path: PathBuf,
    pub pages: Vec<PageDefinition>,
}

impl PageStore {
    pub fn load_or_create<P: AsRef<Path>>(path: P, width: u32, height: u32) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        if !path_buf.exists() {
            let pages = default_pages(width, height);
            let store = Self {
                path: path_buf,
                pages,
            };
            store.save()?;
            return Ok(store);
        }

        let raw = fs::read_to_string(&path_buf)
            .with_context(|| format!("failed to read page store: {}", path_buf.display()))?;
        let (mut pages, mut changed) = parse_page_store_json_with_migrations(&raw)
            .context("failed to parse page store JSON")?;

        changed |= migrate_legacy_seed_page_ids(&mut pages);
        changed |= normalize_page_metadata(&mut pages);

        ensure!(!pages.is_empty(), "page store cannot be empty");

        let store = Self {
            path: path_buf,
            pages,
        };

        if changed {
            store.save()?;
        }

        Ok(store)
    }

    pub fn save(&self) -> Result<()> {
        let raw =
            serde_json::to_string_pretty(&self.pages).context("failed to serialize page store")?;
        fs::write(&self.path, raw)
            .with_context(|| format!("failed to write page store: {}", self.path.display()))?;
        Ok(())
    }

    pub fn list_summaries(&self) -> Vec<PageSummary> {
        self.pages
            .iter()
            .map(|page| PageSummary {
                id: page.id.clone(),
                name: page.name.clone(),
                element_count: page.elements.len(),
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&PageDefinition> {
        self.pages.iter().find(|p| p.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut PageDefinition> {
        self.pages.iter_mut().find(|p| p.id == id)
    }

    pub fn export_page_json_pretty(&self, id: &str) -> Result<String> {
        let page = self
            .get(id)
            .with_context(|| format!("page '{id}' not found"))?;
        serde_json::to_string_pretty(page).context("failed to serialize page export JSON")
    }

    pub fn create_blank_page(&mut self, name: &str, width: u32, height: u32) -> Result<String> {
        let id = unique_page_id(
            &self.pages,
            &conventional_page_id("local", "custom", PAGE_VERSION_DEFAULT, name),
        );
        self.pages.push(PageDefinition {
            id: id.clone(),
            name: name.trim().to_string(),
            width,
            height,
            meta: PageMeta {
                schema_version: PAGE_DEFINITION_SCHEMA_VERSION,
                version: PAGE_VERSION_DEFAULT.to_string(),
                authors: vec!["local".to_string()],
                description: None,
                bundle_name: Some("custom".to_string()),
                license: None,
                source_url: None,
                tags: Vec::new(),
            },
            elements: vec![PageElement::StaticText {
                x: 0,
                y: 10,
                text: "NEW PAGE".to_string(),
                size: TextSize::Large,
                text_height_px: default_text_height_for_size(&TextSize::Large),
                color: MonoColor::On,
                name: None,
            }],
        });
        self.save()?;
        Ok(id)
    }

    pub fn import_page(
        &mut self,
        mut imported: PageDefinition,
        policy: ImportConflictPolicy,
        default_width: u32,
        default_height: u32,
    ) -> Result<String> {
        normalize_page_definition_metadata(&mut imported, default_width, default_height);
        validate_page(&imported)?;

        if let Some(existing_idx) = self.pages.iter().position(|p| p.id == imported.id) {
            match policy {
                ImportConflictPolicy::Error => {
                    bail!("page '{}' already exists", imported.id);
                }
                ImportConflictPolicy::Replace => {
                    self.pages[existing_idx] = imported.clone();
                    self.save()?;
                    return Ok(imported.id);
                }
                ImportConflictPolicy::Duplicate => {
                    let name_for_id = if imported.name.trim().is_empty() {
                        "imported-page"
                    } else {
                        imported.name.as_str()
                    };
                    imported.id = unique_page_id(
                        &self.pages,
                        &conventional_page_id(
                            "imported",
                            imported.meta.bundle_name.as_deref().unwrap_or("bundle"),
                            &imported.meta.version,
                            name_for_id,
                        ),
                    );
                }
            }
        }

        self.pages.push(imported.clone());
        self.save()?;
        Ok(imported.id)
    }

    pub fn rename_page(&mut self, id: &str, name: &str) -> Result<()> {
        let page = self
            .get_mut(id)
            .with_context(|| format!("page '{id}' not found"))?;
        page.name = name.trim().to_string();
        self.save()
    }

    pub fn rekey_page(&mut self, id: &str, new_id: &str) -> Result<()> {
        let new_id = new_id.trim();
        ensure!(!new_id.is_empty(), "new page id cannot be empty");

        if id == new_id {
            return Ok(());
        }

        ensure!(
            !self.pages.iter().any(|page| page.id == new_id),
            "page '{new_id}' already exists"
        );

        let page = self
            .get_mut(id)
            .with_context(|| format!("page '{id}' not found"))?;
        page.id = new_id.to_string();
        self.save()
    }

    pub fn delete_page(&mut self, id: &str) -> Result<()> {
        ensure!(
            self.pages.len() > 1,
            "cannot delete the last remaining page"
        );
        let before = self.pages.len();
        self.pages.retain(|p| p.id != id);
        ensure!(self.pages.len() != before, "page '{id}' not found");
        self.save()
    }

    pub fn replace_page(&mut self, id: &str, mut replacement: PageDefinition) -> Result<()> {
        validate_page(&replacement)?;
        replacement.id = id.to_string();

        let page = self
            .get_mut(id)
            .with_context(|| format!("page '{id}' not found"))?;
        *page = replacement;
        self.save()
    }

    pub fn push_element(&mut self, id: &str, element: PageElement) -> Result<()> {
        let page = self
            .get_mut(id)
            .with_context(|| format!("page '{id}' not found"))?;
        page.elements.push(element);
        self.save()
    }

    pub fn delete_element(&mut self, id: &str, index: usize) -> Result<()> {
        let page = self
            .get_mut(id)
            .with_context(|| format!("page '{id}' not found"))?;
        ensure!(index < page.elements.len(), "element index out of range");
        page.elements.remove(index);
        self.save()
    }
}

pub fn parse_page_definition_from_json_value(mut raw: Value) -> Result<PageDefinition> {
    migrate_legacy_boot_dynamic_sources_in_json_value(&mut raw);
    migrate_legacy_dynamic_source_ids_in_json_value(&mut raw);
    let mut page: PageDefinition =
        serde_json::from_value(raw).context("failed to parse page definition JSON")?;
    let page_width = page.width;
    let page_height = page.height;
    normalize_page_definition_metadata(&mut page, page_width, page_height);
    Ok(page)
}

fn validate_page(page: &PageDefinition) -> Result<()> {
    ensure!(!page.id.trim().is_empty(), "page.id cannot be empty");
    ensure!(!page.name.trim().is_empty(), "page.name cannot be empty");
    ensure!(page.width > 0, "page.width must be > 0");
    ensure!(page.height > 0, "page.height must be > 0");
    ensure!(
        page.meta.schema_version > 0,
        "page.meta.schema_version must be > 0"
    );
    ensure!(
        !page.meta.version.trim().is_empty(),
        "page.meta.version cannot be empty"
    );
    Ok(())
}

fn default_page_schema_version() -> u32 {
    PAGE_DEFINITION_SCHEMA_VERSION
}

fn default_page_version() -> String {
    PAGE_VERSION_DEFAULT.to_string()
}

fn default_text_height_for_size(size: &TextSize) -> u32 {
    match size {
        TextSize::Small => 6,
        TextSize::Large => 10,
    }
}

fn normalized_text_height_px(text_height_px: u32, size: &TextSize) -> u32 {
    if text_height_px == 0 {
        default_text_height_for_size(size)
    } else {
        text_height_px.max(TEXT_HEIGHT_MIN_PX)
    }
}

fn default_pages(width: u32, height: u32) -> Vec<PageDefinition> {
    let mut pages = required_default_runtime_pages(width, height);

    pages.push(PageDefinition {
        id: "solstice-panel-examples-1.0.0-studio-welcome".to_string(),
        name: "Studio Welcome".to_string(),
        width,
        height,
        meta: PageMeta {
            schema_version: PAGE_DEFINITION_SCHEMA_VERSION,
            version: PAGE_VERSION_DEFAULT.to_string(),
            authors: vec!["SOLSTICE Panel".to_string()],
            description: Some("Example starter page.".to_string()),
            bundle_name: Some("examples".to_string()),
            license: None,
            source_url: None,
            tags: vec!["example".to_string()],
        },
        elements: vec![
            PageElement::StaticText {
                x: 0,
                y: 10,
                text: "SOLSTICE".to_string(),
                size: TextSize::Large,
                text_height_px: default_text_height_for_size(&TextSize::Large),
                color: MonoColor::On,
                name: None,
            },
            PageElement::StaticText {
                x: 0,
                y: 22,
                text: "Studio page".to_string(),
                size: TextSize::Small,
                text_height_px: default_text_height_for_size(&TextSize::Small),
                color: MonoColor::On,
                name: None,
            },
        ],
    });

    pages
}

fn parse_page_store_json_with_migrations(raw: &str) -> Result<(Vec<PageDefinition>, bool)> {
    let mut raw_value: Value = serde_json::from_str(raw).context("invalid page-store JSON")?;
    let mut changed = migrate_legacy_boot_dynamic_sources_in_json_value(&mut raw_value);
    changed |= migrate_legacy_dynamic_source_ids_in_json_value(&mut raw_value);
    let pages: Vec<PageDefinition> =
        serde_json::from_value(raw_value).context("invalid page-store page schema")?;
    Ok((pages, changed))
}

fn migrate_legacy_dynamic_source_ids_in_json_value(value: &mut Value) -> bool {
    match value {
        Value::Array(pages) => {
            let mut changed = false;
            for page in pages {
                changed |= migrate_legacy_dynamic_source_ids_in_page_value(page);
            }
            changed
        }
        Value::Object(_) => migrate_legacy_dynamic_source_ids_in_page_value(value),
        _ => false,
    }
}

fn migrate_legacy_dynamic_source_ids_in_page_value(page: &mut Value) -> bool {
    let Some(elements) = page
        .as_object_mut()
        .and_then(|obj| obj.get_mut("elements"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };

    let mut changed = false;
    for element in elements {
        let Some(obj) = element.as_object_mut() else {
            continue;
        };

        if obj.get("type").and_then(Value::as_str) != Some("dynamic_text") {
            continue;
        }

        let Some(source_value) = obj.get_mut("source") else {
            continue;
        };
        let Some(source_str) = source_value.as_str() else {
            continue;
        };

        let Some(canonical) = canonical_dynamic_source_id(source_str) else {
            continue;
        };

        if canonical != source_str {
            *source_value = json!(canonical);
            changed = true;
        }
    }

    changed
}

fn canonical_dynamic_source_id(raw: &str) -> Option<&'static str> {
    Some(match raw {
        "hostname" | "sys:hostname" => "sys:hostname",
        "ip_addr" | "sys:ip_addr" => "sys:ip_addr",
        "local_time_hm" | "sys:local_time_hm" => "sys:local_time_hm",
        "local_time_hms" | "sys:local_time_hms" => "sys:local_time_hms",
        "local_date_ymd" | "sys:local_date_ymd" => "sys:local_date_ymd",
        "local_date_dmy" | "sys:local_date_dmy" => "sys:local_date_dmy",
        "local_date_mdy" | "sys:local_date_mdy" => "sys:local_date_mdy",
        "local_datetime_iso" | "sys:local_datetime_iso" => "sys:local_datetime_iso",
        "local_datetime_compact" | "sys:local_datetime_compact" => "sys:local_datetime_compact",
        "local_datetime_rfc2822" | "sys:local_datetime_rfc2822" => "sys:local_datetime_rfc2822",
        "local_datetime_rfc3339" | "sys:local_datetime_rfc3339" => "sys:local_datetime_rfc3339",
        "uptime_text" | "sys:uptime_text" => "sys:uptime_text",
        "ram_percent_text" | "sys:ram_percent_text" => "sys:ram_percent_text",
        "cpu_temp_text" | "sys:cpu_temp_text" => "sys:cpu_temp_text",
        "cpu_usage_percent" | "sys:cpu_usage_percent" => "sys:cpu_usage_percent",
        "load_avg_1" | "sys:load_avg_1" => "sys:load_avg_1",
        "load_avg_5" | "sys:load_avg_5" => "sys:load_avg_5",
        "load_avg_15" | "sys:load_avg_15" => "sys:load_avg_15",
        "load_avg_text" | "sys:load_avg_text" => "sys:load_avg_text",
        "mem_total_mib" | "sys:mem_total_mib" => "sys:mem_total_mib",
        "mem_used_mib" | "sys:mem_used_mib" => "sys:mem_used_mib",
        "mem_available_mib" | "sys:mem_available_mib" => "sys:mem_available_mib",
        "mem_free_mib" | "sys:mem_free_mib" => "sys:mem_free_mib",
        "swap_total_mib" | "sys:swap_total_mib" => "sys:swap_total_mib",
        "swap_used_mib" | "sys:swap_used_mib" => "sys:swap_used_mib",
        "swap_free_mib" | "sys:swap_free_mib" => "sys:swap_free_mib",
        "swap_used_percent_text" | "sys:swap_used_percent_text" => "sys:swap_used_percent_text",
        "procs_running" | "sys:procs_running" => "sys:procs_running",
        "procs_blocked" | "sys:procs_blocked" => "sys:procs_blocked",
        "cpu_cores" | "sys:cpu_cores" => "sys:cpu_cores",
        "os_pretty_name" | "sys:os_pretty_name" => "sys:os_pretty_name",
        "kernel_release" | "sys:kernel_release" => "sys:kernel_release",
        "active_page" | "display:active_page" => "display:active_page",
        "active_page_id" | "display:active_page_id" => "display:active_page_id",
        "display_mode" | "display:mode" => "display:mode",
        "rotation_active" | "display:rotation_active" => "display:rotation_active",
        "rotation_interval_ms" | "display:rotation_interval_ms" => "display:rotation_interval_ms",
        "rotation_interval_seconds" | "display:rotation_interval_seconds" => {
            "display:rotation_interval_seconds"
        }
        "rotation_queue_len" | "display:rotation_queue_len" => "display:rotation_queue_len",
        "rotation_queue_empty" | "display:rotation_queue_empty" => "display:rotation_queue_empty",
        "rotation_index" | "display:rotation_index" => "display:rotation_index",
        "rotation_next_index" | "display:rotation_next_index" => "display:rotation_next_index",
        "rotation_position" | "display:rotation_position" => "display:rotation_position",
        "display_width" | "display:width" => "display:width",
        "display_height" | "display:height" => "display:height",
        "refresh_ms" | "config:refresh_ms" => "config:refresh_ms",
        "i2c_address" | "config:i2c_address" => "config:i2c_address",
        "i2c_address_hex" | "config:i2c_address_hex" => "config:i2c_address_hex",
        "web_bind" | "config:web_bind" => "config:web_bind",
        "page_id" | "display:page_id" => "display:page_id",
        "page_name" | "display:page_name" => "display:page_name",
        "page_version" | "display:page_version" => "display:page_version",
        "page_authors" | "display:page_authors" => "display:page_authors",
        "page_author_count" | "display:page_author_count" => "display:page_author_count",
        "page_bundle" | "display:page_bundle" => "display:page_bundle",
        "page_license" | "display:page_license" => "display:page_license",
        "page_source_url" | "display:page_source_url" => "display:page_source_url",
        "page_tags" | "display:page_tags" => "display:page_tags",
        "page_tag_count" | "display:page_tag_count" => "display:page_tag_count",
        "page_description" | "display:page_description" => "display:page_description",
        "page_element_count" | "display:page_element_count" => "display:page_element_count",
        "page_width" | "display:page_width" => "display:page_width",
        "page_height" | "display:page_height" => "display:page_height",
        "unix_epoch_seconds" | "sys:unix_epoch_seconds" => "sys:unix_epoch_seconds",
        "unix_epoch_millis" | "sys:unix_epoch_millis" => "sys:unix_epoch_millis",
        _ => return None,
    })
}

fn migrate_legacy_boot_dynamic_sources_in_json_value(value: &mut Value) -> bool {
    match value {
        Value::Array(pages) => {
            let mut changed = false;
            for page in pages {
                changed |= migrate_legacy_boot_dynamic_sources_in_page_value(page);
            }
            changed
        }
        Value::Object(_) => migrate_legacy_boot_dynamic_sources_in_page_value(value),
        _ => false,
    }
}

fn migrate_legacy_boot_dynamic_sources_in_page_value(page: &mut Value) -> bool {
    let Some(elements) = page
        .as_object_mut()
        .and_then(|obj| obj.get_mut("elements"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };

    let mut changed = false;
    for element in elements {
        let Some(obj) = element.as_object() else {
            continue;
        };

        if obj.get("type").and_then(Value::as_str) != Some("dynamic_text") {
            continue;
        }

        let source = obj.get("source").and_then(Value::as_str);
        let replacement_text = match source {
            Some("boot_main") => LEGACY_BOOT_MAIN_TEXT,
            Some("boot_sub") => LEGACY_BOOT_SUB_TEXT,
            _ => continue,
        };

        let x = obj.get("x").cloned().unwrap_or_else(|| json!(0));
        let y = obj.get("y").cloned().unwrap_or_else(|| json!(0));
        let size = obj.get("size").cloned().unwrap_or_else(|| json!("small"));

        *element = json!({
            "type": "static_text",
            "x": x,
            "y": y,
            "text": replacement_text,
            "size": size
        });

        changed = true;
    }

    changed
}

pub fn canonical_page_id(id: &str) -> String {
    match id {
        LEGACY_PAGE_ID_BOOT => PAGE_ID_BOOT.to_string(),
        LEGACY_PAGE_ID_LIVE_INFO | "live-info" => PAGE_ID_LIVE_INFO.to_string(),
        LEGACY_PAGE_ID_DIAGNOSTICS => PAGE_ID_DIAGNOSTICS.to_string(),
        _ => id.to_string(),
    }
}

fn required_default_runtime_pages(width: u32, height: u32) -> Vec<PageDefinition> {
    vec![
        PageDefinition {
            id: PAGE_ID_BOOT.to_string(),
            name: "Boot".to_string(),
            width,
            height,
            meta: PageMeta {
                schema_version: PAGE_DEFINITION_SCHEMA_VERSION,
                version: PAGE_VERSION_DEFAULT.to_string(),
                authors: vec!["SOLSTICE Panel".to_string()],
                description: Some("Boot status page.".to_string()),
                bundle_name: Some("core".to_string()),
                license: None,
                source_url: None,
                tags: vec!["default".to_string(), "runtime".to_string()],
            },
            elements: vec![
                PageElement::StaticText {
                    x: 0,
                    y: 12,
                    text: LEGACY_BOOT_MAIN_TEXT.to_string(),
                    size: TextSize::Large,
                    text_height_px: default_text_height_for_size(&TextSize::Large),
                    color: MonoColor::On,
                    name: None,
                },
                PageElement::StaticText {
                    x: 0,
                    y: 22,
                    text: LEGACY_BOOT_SUB_TEXT.to_string(),
                    size: TextSize::Small,
                    text_height_px: default_text_height_for_size(&TextSize::Small),
                    color: MonoColor::On,
                    name: None,
                },
            ],
        },
        PageDefinition {
            id: PAGE_ID_LIVE_INFO.to_string(),
            name: "Live Info".to_string(),
            width,
            height,
            meta: PageMeta {
                schema_version: PAGE_DEFINITION_SCHEMA_VERSION,
                version: PAGE_VERSION_DEFAULT.to_string(),
                authors: vec!["SOLSTICE Panel".to_string()],
                description: Some("Live runtime metrics page.".to_string()),
                bundle_name: Some("core".to_string()),
                license: None,
                source_url: None,
                tags: vec!["default".to_string(), "runtime".to_string()],
            },
            elements: vec![
                PageElement::DynamicText {
                    x: 0,
                    y: 6,
                    source: DynamicSource::IpAddr,
                    prefix: "IP  ".to_string(),
                    max_chars: 20,
                    size: TextSize::Small,
                    text_height_px: default_text_height_for_size(&TextSize::Small),
                    color: MonoColor::On,
                    name: None,
                },
                PageElement::DynamicText {
                    x: 0,
                    y: 14,
                    source: DynamicSource::CpuTempText,
                    prefix: "CPU ".to_string(),
                    max_chars: 20,
                    size: TextSize::Small,
                    text_height_px: default_text_height_for_size(&TextSize::Small),
                    color: MonoColor::On,
                    name: None,
                },
                PageElement::DynamicText {
                    x: 0,
                    y: 22,
                    source: DynamicSource::UptimeText,
                    prefix: "UP  ".to_string(),
                    max_chars: 20,
                    size: TextSize::Small,
                    text_height_px: default_text_height_for_size(&TextSize::Small),
                    color: MonoColor::On,
                    name: None,
                },
                PageElement::DynamicText {
                    x: 0,
                    y: 30,
                    source: DynamicSource::RamPercentText,
                    prefix: "RAM ".to_string(),
                    max_chars: 20,
                    size: TextSize::Small,
                    text_height_px: default_text_height_for_size(&TextSize::Small),
                    color: MonoColor::On,
                    name: None,
                },
            ],
        },
        PageDefinition {
            id: PAGE_ID_DIAGNOSTICS.to_string(),
            name: "Diagnostics".to_string(),
            width,
            height,
            meta: PageMeta {
                schema_version: PAGE_DEFINITION_SCHEMA_VERSION,
                version: PAGE_VERSION_DEFAULT.to_string(),
                authors: vec!["SOLSTICE Panel".to_string()],
                description: Some("Diagnostics summary page.".to_string()),
                bundle_name: Some("core".to_string()),
                license: None,
                source_url: None,
                tags: vec!["default".to_string(), "runtime".to_string()],
            },
            elements: vec![
                PageElement::StaticText {
                    x: 0,
                    y: 6,
                    text: "SOLSTICE DIAG".to_string(),
                    size: TextSize::Small,
                    text_height_px: default_text_height_for_size(&TextSize::Small),
                    color: MonoColor::On,
                    name: None,
                },
                PageElement::DynamicText {
                    x: 0,
                    y: 14,
                    source: DynamicSource::Hostname,
                    prefix: "HOST ".to_string(),
                    max_chars: 16,
                    size: TextSize::Small,
                    text_height_px: default_text_height_for_size(&TextSize::Small),
                    color: MonoColor::On,
                    name: None,
                },
                PageElement::DynamicText {
                    x: 0,
                    y: 22,
                    source: DynamicSource::IpAddr,
                    prefix: "IP   ".to_string(),
                    max_chars: 16,
                    size: TextSize::Small,
                    text_height_px: default_text_height_for_size(&TextSize::Small),
                    color: MonoColor::On,
                    name: None,
                },
                PageElement::DynamicText {
                    x: 0,
                    y: 30,
                    source: DynamicSource::CpuTempText,
                    prefix: "TEMP ".to_string(),
                    max_chars: 16,
                    size: TextSize::Small,
                    text_height_px: default_text_height_for_size(&TextSize::Small),
                    color: MonoColor::On,
                    name: None,
                },
            ],
        },
    ]
}

fn normalize_page_metadata(pages: &mut [PageDefinition]) -> bool {
    let mut changed = false;
    for page in pages {
        changed |= normalize_page_definition_metadata(page, page.width, page.height);
    }
    changed
}

fn normalize_page_definition_metadata(
    page: &mut PageDefinition,
    default_width: u32,
    default_height: u32,
) -> bool {
    let mut changed = false;

    if page.width == 0 {
        page.width = default_width.max(1);
        changed = true;
    }
    if page.height == 0 {
        page.height = default_height.max(1);
        changed = true;
    }

    if page.meta.schema_version == 0 {
        page.meta.schema_version = PAGE_DEFINITION_SCHEMA_VERSION;
        changed = true;
    }
    if page.meta.version.trim().is_empty() {
        page.meta.version = PAGE_VERSION_DEFAULT.to_string();
        changed = true;
    }
    if page.meta.bundle_name.is_none() {
        let default_bundle = if matches!(
            page.id.as_str(),
            PAGE_ID_BOOT | PAGE_ID_LIVE_INFO | PAGE_ID_DIAGNOSTICS
        ) {
            "core"
        } else {
            "custom"
        };
        page.meta.bundle_name = Some(default_bundle.to_string());
        changed = true;
    }
    if page.meta.authors.is_empty()
        && matches!(
            page.id.as_str(),
            PAGE_ID_BOOT | PAGE_ID_LIVE_INFO | PAGE_ID_DIAGNOSTICS
        )
    {
        page.meta.authors = vec!["SOLSTICE Panel".to_string()];
        changed = true;
    }

    for element in &mut page.elements {
        match element {
            PageElement::StaticText {
                size,
                text_height_px,
                ..
            }
            | PageElement::DynamicText {
                size,
                text_height_px,
                ..
            } => {
                let normalized = normalized_text_height_px(*text_height_px, size);
                if *text_height_px != normalized {
                    *text_height_px = normalized;
                    changed = true;
                }
            }
            PageElement::Image { w, h, source, .. } => {
                if *w == 0 {
                    *w = 1;
                    changed = true;
                }
                if *h == 0 {
                    *h = 1;
                    changed = true;
                }
                let trimmed = source.trim().to_string();
                if *source != trimmed {
                    *source = trimmed;
                    changed = true;
                }
            }
            _ => {}
        }
    }

    changed
}

fn migrate_legacy_seed_page_ids(pages: &mut [PageDefinition]) -> bool {
    let mut changed = false;
    for (legacy_ids, canonical_id) in [
        (&[LEGACY_PAGE_ID_BOOT][..], PAGE_ID_BOOT),
        (
            &[LEGACY_PAGE_ID_LIVE_INFO, "live-info"][..],
            PAGE_ID_LIVE_INFO,
        ),
        (&[LEGACY_PAGE_ID_DIAGNOSTICS][..], PAGE_ID_DIAGNOSTICS),
    ] {
        if pages.iter().any(|p| p.id == canonical_id) {
            continue;
        }

        for legacy_id in legacy_ids {
            if let Some(page) = pages.iter_mut().find(|p| p.id == *legacy_id) {
                page.id = canonical_id.to_string();
                changed = true;
                break;
            }
        }
    }
    changed
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;

    for ch in input.trim().to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }

    out.trim_matches('-').to_string()
}

fn conventional_page_id(author: &str, bundle_name: &str, version: &str, name: &str) -> String {
    let author = {
        let slug = slugify(author);
        if slug.is_empty() {
            "unknown".to_string()
        } else {
            slug
        }
    };
    let bundle_name = {
        let slug = slugify(bundle_name);
        if slug.is_empty() {
            "bundle".to_string()
        } else {
            slug
        }
    };
    let version = {
        let mut out = String::new();
        for ch in version.trim().chars() {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' {
                out.push(ch.to_ascii_lowercase());
            }
        }
        if out.is_empty() {
            PAGE_VERSION_DEFAULT.to_string()
        } else {
            out
        }
    };
    let name = {
        let slug = slugify(name);
        if slug.is_empty() {
            "page".to_string()
        } else {
            slug
        }
    };

    format!("{author}-{bundle_name}-{version}-{name}")
}

fn sanitize_page_id(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.trim().to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }

    out.trim_matches('-').to_string()
}

fn unique_page_id(existing: &[PageDefinition], base: &str) -> String {
    let base = {
        let normalized = sanitize_page_id(base);
        if normalized.is_empty() {
            "page".to_string()
        } else {
            normalized
        }
    };

    if !existing.iter().any(|p| p.id == base) {
        return base;
    }

    for i in 2..10_000 {
        let candidate = format!("{base}-{i}");
        if !existing.iter().any(|p| p.id == candidate) {
            return candidate;
        }
    }

    format!("{base}-overflow")
}
