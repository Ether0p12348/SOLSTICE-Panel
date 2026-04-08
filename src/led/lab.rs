use anyhow::{Context, Result, ensure};
use embedded_hal::i2c::I2c;
use linux_embedded_hal::I2cdev;
use serde::{Deserialize, Serialize, de::Unexpected};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, RwLock};

pub const LED_LAB_BUS: u8 = 7;
pub const LED_LAB_DEVICE_ADDRESS: u8 = 0x0e;
pub const OLED_DEVICE_ADDRESS: u8 = 0x3c;

const LED_LAB_MAX_STEP_DELAY_MS: u64 = 5_000;
const LED_LAB_MAX_STEPS_PER_ENTRY: usize = 64;
const LED_LAB_MAX_STEPS_PER_RUN: usize = 64;
const LED_LAB_MIN_WRITE_SPACING_MS: u64 = 45;

pub type SharedLedLabStore = Arc<RwLock<LedLabStore>>;
pub type SharedLedLabRunner = Arc<LedLabRunner>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LedLabConfidence {
    Confirmed,
    Likely,
    #[default]
    Unknown,
    Conflicting,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LedLabCategory {
    Fan,
    BuiltinEffect,
    BuiltinSpeed,
    BuiltinColor,
    DirectMode,
    DirectChannel,
    #[default]
    Unknown,
}

impl LedLabCategory {
    pub fn as_id(self) -> &'static str {
        match self {
            Self::Fan => "fan",
            Self::BuiltinEffect => "builtin_effect",
            Self::BuiltinSpeed => "builtin_speed",
            Self::BuiltinColor => "builtin_color",
            Self::DirectMode => "direct_mode",
            Self::DirectChannel => "direct_channel",
            Self::Unknown => "unknown",
        }
    }

    pub fn as_label(self) -> &'static str {
        match self {
            Self::Fan => "Fan Commands",
            Self::BuiltinEffect => "Built-in Effect Table",
            Self::BuiltinSpeed => "Built-in Speed Table",
            Self::BuiltinColor => "Built-in Color Table",
            Self::DirectMode => "Direct Mode Control",
            Self::DirectChannel => "Direct Mode Channels",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LedLabCommandClass {
    FanControl,
    #[serde(alias = "builtin_value_table", alias = "builtin_single_register")]
    BuiltinEffect,
    BuiltinSpeed,
    BuiltinColor,
    #[serde(alias = "direct_sequence_control")]
    DirectMode,
    #[serde(alias = "direct_value_parameter")]
    DirectChannel,
    #[default]
    Generic,
}

impl LedLabCommandClass {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::FanControl => "Fan Control",
            Self::BuiltinEffect => "Built-in Effect",
            Self::BuiltinSpeed => "Built-in Speed",
            Self::BuiltinColor => "Built-in Color",
            Self::DirectMode => "Direct Mode",
            Self::DirectChannel => "Direct Channel",
            Self::Generic => "Generic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedLabStep {
    pub register: u8,
    pub value: u8,
    #[serde(default)]
    pub delay_ms: u64,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedLabEntry {
    pub id: String,
    #[serde(default, skip_serializing)]
    pub category: LedLabCategory,
    #[serde(default)]
    pub command_class: LedLabCommandClass,
    #[serde(default = "default_bus")]
    pub bus: u8,
    #[serde(
        default = "default_device_address",
        deserialize_with = "deserialize_u8_hex_or_int",
        serialize_with = "serialize_u8_as_hex"
    )]
    pub device_address: u8,
    #[serde(
        deserialize_with = "deserialize_u8_hex_or_int",
        serialize_with = "serialize_u8_as_hex"
    )]
    pub register: u8,
    #[serde(
        deserialize_with = "deserialize_u8_hex_or_int",
        serialize_with = "serialize_u8_as_hex"
    )]
    pub value: u8,
    #[serde(default, skip_serializing)]
    pub sequence_context: Option<String>,
    #[serde(default, alias = "machine-name", skip_serializing)]
    pub machine_name: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub observed_behavior: String,
    #[serde(default, skip_serializing)]
    pub confidence: LedLabConfidence,
    #[serde(default, skip_serializing)]
    pub notes: String,
    #[serde(default, skip_serializing)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing)]
    pub steps: Vec<LedLabStep>,
    #[serde(default, alias = "bash_equivalent", rename = "bash-equivalent")]
    pub bash_equivalent: String,
    #[serde(default, skip_serializing)]
    pub created_at: String,
    #[serde(default, skip_serializing)]
    pub updated_at: String,
    #[serde(default)]
    pub can_speed: bool,
    #[serde(default)]
    pub can_color_preset: bool,
    #[serde(default)]
    pub can_index: bool,
    #[serde(default)]
    pub can_color_24: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedLabEntryDraft {
    pub id: Option<String>,
    #[serde(default)]
    pub category: LedLabCategory,
    #[serde(default)]
    pub command_class: LedLabCommandClass,
    #[serde(default = "default_bus")]
    pub bus: u8,
    #[serde(default = "default_device_address")]
    pub device_address: u8,
    pub register: u8,
    pub value: u8,
    #[serde(default)]
    pub sequence_context: Option<String>,
    pub machine_name: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub observed_behavior: String,
    #[serde(default)]
    pub confidence: LedLabConfidence,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub steps: Vec<LedLabStep>,
    #[serde(default)]
    pub observation_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedStudioModeConfig {
    pub id: String,
    pub mode_name: String,
    #[serde(default)]
    pub mode_description: String,
    pub registration: String,
    #[serde(default)]
    pub speed_customization_available: bool,
    #[serde(default)]
    pub color_preset_customization_available: bool,
    #[serde(default)]
    pub can_index: bool,
    #[serde(default)]
    pub can_color_24: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LedLabStoreFile {
    #[serde(default)]
    schema_version: Option<u32>,
    commands: Vec<LedLabEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyLedLabStoreFile {
    #[serde(default)]
    entries: Vec<LegacyLedLabEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyLedLabEntry {
    id: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    notes: String,
    #[serde(default = "default_bus")]
    bus: u8,
    #[serde(default = "default_device_address")]
    device_address: u8,
    #[serde(default)]
    steps: Vec<LedLabStep>,
    #[serde(default)]
    observed_behavior: String,
    #[serde(default)]
    observed_result: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
}

#[derive(Debug, Clone)]
pub struct LedLabStore {
    pub path: PathBuf,
    pub entries: Vec<LedLabEntry>,
    pub studio_modes: Vec<LedStudioModeConfig>,
}

impl LedLabStore {
    pub fn load_or_create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        if !path_buf.exists() {
            let entries = seeded_commands();
            let store = Self {
                path: path_buf,
                studio_modes: studio_modes_from_entries(&entries),
                entries,
            };
            store.save()?;
            return Ok(store);
        }

        let raw = fs::read_to_string(&path_buf)
            .with_context(|| format!("failed reading LED lab store: {}", path_buf.display()))?;

        let value: Value = serde_json::from_str(&raw).context("failed parsing LED lab JSON")?;
        let mut entries = if value.get("commands").is_some() {
            let parsed: LedLabStoreFile =
                serde_json::from_value(value).context("failed parsing LED lab v2 JSON")?;
            parsed.commands
        } else {
            let parsed: LegacyLedLabStoreFile =
                serde_json::from_str(&raw).context("failed parsing legacy LED lab JSON")?;
            migrate_legacy_entries(parsed.entries)
        };

        normalize_entries(&mut entries)?;
        let studio_modes = studio_modes_from_entries(&entries);

        Ok(Self {
            path: path_buf,
            entries,
            studio_modes,
        })
    }

    pub fn save(&self) -> Result<()> {
        let mut root = serde_json::Map::new();
        root.insert(
            "commands".to_string(),
            serde_json::to_value(&self.entries).context("failed serializing LED lab commands")?,
        );

        let raw = serde_json::to_string_pretty(&Value::Object(root))
            .context("failed serializing LED lab store")?;
        fs::write(&self.path, raw)
            .with_context(|| format!("failed writing LED lab store: {}", self.path.display()))?;
        Ok(())
    }

    pub fn get_entry(&self, id: &str) -> Option<&LedLabEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }
}

fn migrate_legacy_entries(legacy_entries: Vec<LegacyLedLabEntry>) -> Vec<LedLabEntry> {
    let mut entries = seeded_commands();

    for legacy in legacy_entries {
        let first_step = legacy.steps.first().cloned().unwrap_or(LedLabStep {
            register: 0,
            value: 0,
            delay_ms: 0,
            note: String::new(),
        });

        let machine_name =
            sanitize_machine_name(&legacy.label, first_step.register, first_step.value);
        let created_at = if legacy.created_at.trim().is_empty() {
            now_rfc3339()
        } else {
            legacy.created_at
        };
        let updated_at = if legacy.updated_at.trim().is_empty() {
            created_at.clone()
        } else {
            legacy.updated_at
        };

        entries.push(LedLabEntry {
            id: generate_entry_id(&legacy.id, &entries),
            category: LedLabCategory::Unknown,
            command_class: LedLabCommandClass::Generic,
            bus: legacy.bus,
            device_address: legacy.device_address,
            register: first_step.register,
            value: first_step.value,
            sequence_context: None,
            machine_name,
            label: if legacy.label.trim().is_empty() {
                format!(
                    "legacy reg 0x{:02x} val 0x{:02x}",
                    first_step.register, first_step.value
                )
            } else {
                legacy.label
            },
            description: String::new(),
            observed_behavior: legacy.observed_behavior,
            confidence: LedLabConfidence::Unknown,
            notes: if legacy.observed_result.trim().is_empty() {
                legacy.notes
            } else if legacy.notes.trim().is_empty() {
                legacy.observed_result
            } else {
                format!("{}\n{}", legacy.notes, legacy.observed_result)
            },
            tags: sanitize_tags(legacy.tags),
            steps: normalize_steps(legacy.steps),
            bash_equivalent: String::new(),
            created_at,
            updated_at,
            can_speed: false,
            can_color_preset: false,
            can_index: false,
            can_color_24: false,
        });
    }

    entries
}

fn normalize_entries(entries: &mut Vec<LedLabEntry>) -> Result<bool> {
    let mut changed = false;

    for entry in entries {
        let before = serde_json::to_string(entry).unwrap_or_default();
        normalize_entry(entry)?;
        let after = serde_json::to_string(entry).unwrap_or_default();
        if before != after {
            changed = true;
        }
    }

    Ok(changed)
}

fn normalize_entry(entry: &mut LedLabEntry) -> Result<()> {
    ensure!(!entry.id.trim().is_empty(), "entry id cannot be empty");
    ensure!(
        entry.bus == LED_LAB_BUS,
        "entry bus must be {}",
        LED_LAB_BUS
    );
    ensure!(
        entry.device_address == LED_LAB_DEVICE_ADDRESS,
        "entry device address must be 0x{:02x}",
        LED_LAB_DEVICE_ADDRESS
    );

    entry.machine_name = sanitize_machine_name(&entry.machine_name, entry.register, entry.value);
    if entry.machine_name.is_empty() {
        entry.machine_name = entry.id.clone();
    }
    if entry.command_class == LedLabCommandClass::BuiltinEffect && entry.register == 0x05 {
        entry.command_class = LedLabCommandClass::BuiltinSpeed;
    } else if entry.command_class == LedLabCommandClass::BuiltinEffect && entry.register == 0x06 {
        entry.command_class = LedLabCommandClass::BuiltinColor;
    } else if entry.command_class == LedLabCommandClass::Generic && entry.register == 0x08 {
        entry.command_class = LedLabCommandClass::FanControl;
    }
    entry.category = category_for_entry(entry);
    ensure!(
        !entry.label.trim().is_empty(),
        "entry label cannot be empty"
    );

    if entry.steps.is_empty() {
        entry.steps = vec![LedLabStep {
            register: entry.register,
            value: entry.value,
            delay_ms: 0,
            note: String::new(),
        }];
    }
    entry.steps = normalize_steps(entry.steps.clone());
    validate_steps(&entry.steps)?;

    entry.tags = sanitize_tags(entry.tags.clone());
    entry.description = entry.description.trim().to_string();
    entry.observed_behavior = entry.observed_behavior.trim().to_string();
    entry.notes = entry.notes.trim().to_string();
    entry.sequence_context = normalized_optional_text(entry.sequence_context.clone());

    if entry.created_at.trim().is_empty() {
        entry.created_at = now_rfc3339();
    }
    if entry.updated_at.trim().is_empty() {
        entry.updated_at = entry.created_at.clone();
    }

    entry.bash_equivalent =
        bash_preview_for_steps(entry.bus, entry.device_address, &entry.steps).join("\n");

    Ok(())
}

fn normalized_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn sanitize_machine_name(raw: &str, register: u8, value: u8) -> String {
    let slug = slugify(raw);
    if slug == "entry" {
        format!("reg-{register:02x}-val-{value:02x}")
    } else {
        slug
    }
}

fn sanitize_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for tag in tags {
        let cleaned = tag.trim().to_string();
        if cleaned.is_empty() {
            continue;
        }
        if seen.insert(cleaned.clone()) {
            out.push(cleaned);
        }
    }
    out
}

fn default_bus() -> u8 {
    LED_LAB_BUS
}

fn default_device_address() -> u8 {
    LED_LAB_DEVICE_ADDRESS
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn generate_entry_id(base: &str, existing: &[LedLabEntry]) -> String {
    let mut id = slugify(base);
    if id == "entry" {
        id = format!(
            "led-command-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
    }

    if !existing.iter().any(|entry| entry.id == id) {
        return id;
    }

    let mut idx = 2u32;
    loop {
        let candidate = format!("{id}-{idx}");
        if !existing.iter().any(|entry| entry.id == candidate) {
            return candidate;
        }
        idx = idx.saturating_add(1);
    }
}

fn slugify(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "entry".to_string()
    } else {
        trimmed
    }
}

fn studio_modes_from_entries(entries: &[LedLabEntry]) -> Vec<LedStudioModeConfig> {
    let mut modes: Vec<LedStudioModeConfig> = entries
        .iter()
        .filter(|entry| {
            entry.command_class == LedLabCommandClass::BuiltinEffect && entry.register == 0x04
        })
        .map(|entry| LedStudioModeConfig {
            id: entry.id.clone(),
            mode_name: entry.label.clone(),
            mode_description: entry.description.clone(),
            registration: format!("0x{:02x}=0x{:02x}", entry.register, entry.value),
            speed_customization_available: entry.can_speed,
            color_preset_customization_available: entry.can_color_preset,
            can_index: entry.can_index,
            can_color_24: entry.can_color_24,
        })
        .collect();
    modes.sort_by(|a, b| a.id.cmp(&b.id));
    modes
}

fn category_for_entry(entry: &LedLabEntry) -> LedLabCategory {
    match entry.command_class {
        LedLabCommandClass::FanControl => LedLabCategory::Fan,
        LedLabCommandClass::BuiltinEffect => LedLabCategory::BuiltinEffect,
        LedLabCommandClass::BuiltinSpeed => LedLabCategory::BuiltinSpeed,
        LedLabCommandClass::BuiltinColor => LedLabCategory::BuiltinColor,
        LedLabCommandClass::DirectMode => LedLabCategory::DirectMode,
        LedLabCommandClass::DirectChannel => LedLabCategory::DirectChannel,
        LedLabCommandClass::Generic => LedLabCategory::Unknown,
    }
}

fn serialize_u8_as_hex<S>(value: &u8, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&format!("0x{value:02x}"))
}

fn deserialize_u8_hex_or_int<'de, D>(deserializer: D) -> std::result::Result<u8, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum WireValue {
        Number(u64),
        Text(String),
    }

    match WireValue::deserialize(deserializer)? {
        WireValue::Number(raw) => u8::try_from(raw).map_err(|_| {
            serde::de::Error::invalid_value(
                Unexpected::Unsigned(raw),
                &"an integer between 0 and 255",
            )
        }),
        WireValue::Text(raw) => {
            let trimmed = raw.trim();
            let cleaned = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
                .unwrap_or(trimmed);
            u8::from_str_radix(cleaned, 16).map_err(|_| {
                serde::de::Error::invalid_value(
                    Unexpected::Str(trimmed),
                    &"a hex string like 0x0e or an integer",
                )
            })
        }
    }
}

fn seeded_commands() -> Vec<LedLabEntry> {
    let now = now_rfc3339();
    let mut out = Vec::new();

    // Fan command map.
    out.push(seed_single_command(
        "fan-off",
        LedLabCategory::Fan,
        LedLabCommandClass::FanControl,
        0x08,
        0x00,
        "Fan Off",
        "Yahboom fan control register. 0x00 disables the fan.",
        LedLabConfidence::Confirmed,
        None,
        false,
        false,
        false,
        false,
        now.clone(),
    ));
    out.push(seed_single_command(
        "fan-on",
        LedLabCategory::Fan,
        LedLabCommandClass::FanControl,
        0x08,
        0x01,
        "Fan On",
        "Yahboom fan control register. 0x01 enables the fan.",
        LedLabConfidence::Confirmed,
        None,
        false,
        false,
        false,
        false,
        now.clone(),
    ));

    // Built-in effect table (known values 0x00..0x06).
    for value in 0x00_u8..=0x06_u8 {
        let label = if value == 0x00 {
            "Built-in Effect 0 / Direct Mode Arm"
        } else {
            "Built-in Effect"
        };
        let description = if value == 0x00 {
            "Effect selector register 0x04. Value 0x00 is known to overlap with direct mode enable."
        } else {
            "Effect selector register 0x04. Human effect name still needs observation labeling."
        };
        out.push(seed_single_command(
            &format!("effect-{:02x}", value),
            LedLabCategory::BuiltinEffect,
            LedLabCommandClass::BuiltinEffect,
            0x04,
            value,
            &format!("{} #{:02X}", label, value),
            description,
            if value == 0x00 {
                LedLabConfidence::Confirmed
            } else {
                LedLabConfidence::Likely
            },
            None,
            value != 0x00,
            value != 0x00,
            value == 0x00,
            value == 0x00,
            now.clone(),
        ));
    }

    // Built-in speed table (known values 0x01..0x03).
    for value in 0x01_u8..=0x03_u8 {
        out.push(seed_single_command(
            &format!("speed-{:02x}", value),
            LedLabCategory::BuiltinSpeed,
            LedLabCommandClass::BuiltinSpeed,
            0x05,
            value,
            &format!("Built-in Speed #{:02X}", value),
            "Speed selector register 0x05. Value names still need visual labeling.",
            LedLabConfidence::Likely,
            None,
            false,
            false,
            false,
            false,
            now.clone(),
        ));
    }

    // Built-in color preset table (known values 0x00..0x06).
    for value in 0x00_u8..=0x06_u8 {
        out.push(seed_single_command(
            &format!("color-{:02x}", value),
            LedLabCategory::BuiltinColor,
            LedLabCommandClass::BuiltinColor,
            0x06,
            value,
            &format!("Built-in Color #{:02X}", value),
            "Color selector register 0x06. Preset color names still need visual labeling.",
            LedLabConfidence::Likely,
            None,
            false,
            false,
            false,
            false,
            now.clone(),
        ));
    }

    // Direct mode control + parameter registers.
    out.push(seed_single_command(
        "direct-mode-enable",
        LedLabCategory::DirectMode,
        LedLabCommandClass::DirectMode,
        0x04,
        0x00,
        "Enable Direct Per-LED Mode",
        "Set 0x04=0x00 before writing LED index and RGB registers.",
        LedLabConfidence::Confirmed,
        Some("direct_single_color".to_string()),
        false,
        false,
        false,
        false,
        now.clone(),
    ));

    out.push(seed_single_command(
        "direct-index-register",
        LedLabCategory::DirectChannel,
        LedLabCommandClass::DirectChannel,
        0x00,
        0x00,
        "Direct Mode LED Index Register",
        "Write LED index (0..N) to register 0x00 after direct mode enable.",
        LedLabConfidence::Confirmed,
        Some("direct_single_color".to_string()),
        false,
        false,
        false,
        false,
        now.clone(),
    ));
    out.push(seed_single_command(
        "direct-red-register",
        LedLabCategory::DirectChannel,
        LedLabCommandClass::DirectChannel,
        0x01,
        0x00,
        "Direct Mode Red Register",
        "Write red channel (0..255) to register 0x01 in direct mode sequence.",
        LedLabConfidence::Confirmed,
        Some("direct_single_color".to_string()),
        false,
        false,
        false,
        false,
        now.clone(),
    ));
    out.push(seed_single_command(
        "direct-green-register",
        LedLabCategory::DirectChannel,
        LedLabCommandClass::DirectChannel,
        0x02,
        0x00,
        "Direct Mode Green Register",
        "Write green channel (0..255) to register 0x02 in direct mode sequence.",
        LedLabConfidence::Confirmed,
        Some("direct_single_color".to_string()),
        false,
        false,
        false,
        false,
        now.clone(),
    ));
    out.push(seed_single_command(
        "direct-blue-register",
        LedLabCategory::DirectChannel,
        LedLabCommandClass::DirectChannel,
        0x03,
        0x00,
        "Direct Mode Blue Register",
        "Write blue channel (0..255) to register 0x03 in direct mode sequence.",
        LedLabConfidence::Confirmed,
        Some("direct_single_color".to_string()),
        false,
        false,
        false,
        false,
        now,
    ));

    out
}

fn seed_single_command(
    id: &str,
    category: LedLabCategory,
    command_class: LedLabCommandClass,
    register: u8,
    value: u8,
    label: &str,
    description: &str,
    confidence: LedLabConfidence,
    sequence_context: Option<String>,
    can_speed: bool,
    can_color_preset: bool,
    can_index: bool,
    can_color_24: bool,
    timestamp: String,
) -> LedLabEntry {
    let steps = vec![LedLabStep {
        register,
        value,
        delay_ms: 0,
        note: String::new(),
    }];

    LedLabEntry {
        id: id.to_string(),
        category,
        command_class,
        bus: LED_LAB_BUS,
        device_address: LED_LAB_DEVICE_ADDRESS,
        register,
        value,
        sequence_context,
        machine_name: id.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        observed_behavior: String::new(),
        confidence,
        notes: String::new(),
        tags: Vec::new(),
        steps: steps.clone(),
        bash_equivalent: bash_preview_for_steps(LED_LAB_BUS, LED_LAB_DEVICE_ADDRESS, &steps)
            .join("\n"),
        created_at: timestamp.clone(),
        updated_at: timestamp,
        can_speed,
        can_color_preset,
        can_index,
        can_color_24,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LedLabKnownFacts {
    pub bus: u8,
    pub led_device_address: u8,
    pub led_device_hex: String,
    pub oled_device_address: u8,
    pub oled_device_hex: String,
    pub shared_bus_caution: String,
    pub i2cdetect_note: String,
}

pub fn known_facts() -> LedLabKnownFacts {
    LedLabKnownFacts {
        bus: LED_LAB_BUS,
        led_device_address: LED_LAB_DEVICE_ADDRESS,
        led_device_hex: format!("0x{LED_LAB_DEVICE_ADDRESS:02x}"),
        oled_device_address: OLED_DEVICE_ADDRESS,
        oled_device_hex: format!("0x{OLED_DEVICE_ADDRESS:02x}"),
        shared_bus_caution:
            "OLED and LED controller share bus 7. Keep command tests deliberate and low-rate."
                .to_string(),
        i2cdetect_note: "Known endpoints from i2cdetect: 0x0e (LED controller), 0x3c (OLED)."
            .to_string(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LedLabRunnerSnapshot {
    pub running: bool,
    pub abort_requested: bool,
    pub current_label: Option<String>,
    pub started_at: Option<String>,
    pub last_finished_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug)]
struct LedLabRunnerState {
    running: bool,
    current_label: Option<String>,
    started_at: Option<String>,
    last_finished_at: Option<String>,
    last_error: Option<String>,
    last_write_at: Option<Instant>,
}

pub struct LedLabRunner {
    i2c_path: String,
    bus: u8,
    device_address: u8,
    min_write_spacing: Duration,
    run_lock: Mutex<()>,
    state: Mutex<LedLabRunnerState>,
    abort_requested: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LedLabStepResult {
    pub register: u8,
    pub value: u8,
    pub delay_ms: u64,
    pub success: bool,
    pub error: Option<String>,
    pub executed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LedLabRunResult {
    pub started_at: String,
    pub finished_at: String,
    pub success: bool,
    pub aborted: bool,
    pub steps_attempted: usize,
    pub steps_completed: usize,
    pub error: Option<String>,
    pub step_results: Vec<LedLabStepResult>,
    pub bash_commands: Vec<String>,
}

struct BlockingRunOutput {
    result: LedLabRunResult,
    last_write_at: Option<Instant>,
}

impl LedLabRunner {
    pub fn new() -> Self {
        Self {
            i2c_path: format!("/dev/i2c-{LED_LAB_BUS}"),
            bus: LED_LAB_BUS,
            device_address: LED_LAB_DEVICE_ADDRESS,
            min_write_spacing: Duration::from_millis(LED_LAB_MIN_WRITE_SPACING_MS),
            run_lock: Mutex::new(()),
            state: Mutex::new(LedLabRunnerState {
                running: false,
                current_label: None,
                started_at: None,
                last_finished_at: None,
                last_error: None,
                last_write_at: None,
            }),
            abort_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn snapshot(&self) -> LedLabRunnerSnapshot {
        let state = self.state.lock().await;
        LedLabRunnerSnapshot {
            running: state.running,
            abort_requested: self.abort_requested.load(Ordering::SeqCst),
            current_label: state.current_label.clone(),
            started_at: state.started_at.clone(),
            last_finished_at: state.last_finished_at.clone(),
            last_error: state.last_error.clone(),
        }
    }

    pub async fn request_abort(&self) -> bool {
        self.abort_requested.store(true, Ordering::SeqCst);
        self.state.lock().await.running
    }

    pub async fn execute_steps(&self, label: &str, steps: Vec<LedLabStep>) -> LedLabRunResult {
        let label = label.trim().to_string();
        if label.is_empty() {
            return immediate_failure(
                "run label cannot be empty",
                &steps,
                self.bus,
                self.device_address,
            );
        }

        if let Err(err) = validate_steps(&steps) {
            return immediate_failure(&err.to_string(), &steps, self.bus, self.device_address);
        }

        if steps.len() > LED_LAB_MAX_STEPS_PER_RUN {
            return immediate_failure(
                &format!("step count cannot exceed {LED_LAB_MAX_STEPS_PER_RUN}"),
                &steps,
                self.bus,
                self.device_address,
            );
        }

        let _run_guard = self.run_lock.lock().await;
        self.abort_requested.store(false, Ordering::SeqCst);

        let started_at = now_rfc3339();
        let last_write_at = {
            let mut state = self.state.lock().await;
            state.running = true;
            state.current_label = Some(label);
            state.started_at = Some(started_at);
            state.last_error = None;
            state.last_write_at
        };

        let path = self.i2c_path.clone();
        let address = self.device_address;
        let bus = self.bus;
        let min_spacing = self.min_write_spacing;
        let abort_flag = self.abort_requested.clone();
        let steps_for_run = normalize_steps(steps.clone());

        let join_result = tokio::task::spawn_blocking(move || {
            execute_steps_blocking(
                path,
                bus,
                address,
                min_spacing,
                last_write_at,
                steps_for_run,
                abort_flag,
            )
        })
        .await;

        let mut output = match join_result {
            Ok(output) => output,
            Err(err) => BlockingRunOutput {
                result: immediate_failure(
                    &format!("internal execution task failed: {err}"),
                    &steps,
                    self.bus,
                    self.device_address,
                ),
                last_write_at,
            },
        };

        output.result.bash_commands = bash_preview_for_steps(self.bus, self.device_address, &steps);

        {
            let mut state = self.state.lock().await;
            state.running = false;
            state.last_finished_at = Some(output.result.finished_at.clone());
            state.last_error = output.result.error.clone();
            state.last_write_at = output.last_write_at;
            state.current_label = None;
            state.started_at = None;
        }

        output.result
    }
}

fn execute_steps_blocking(
    i2c_path: String,
    bus: u8,
    address: u8,
    min_spacing: Duration,
    mut last_write_at: Option<Instant>,
    steps: Vec<LedLabStep>,
    abort_requested: Arc<AtomicBool>,
) -> BlockingRunOutput {
    let started_at = now_rfc3339();
    let mut step_results = Vec::with_capacity(steps.len());
    let mut steps_attempted = 0usize;
    let mut steps_completed = 0usize;
    let mut aborted = false;
    let mut run_error: Option<String> = None;

    let mut i2c = match I2cdev::new(&i2c_path) {
        Ok(i2c) => i2c,
        Err(err) => {
            return BlockingRunOutput {
                result: LedLabRunResult {
                    started_at,
                    finished_at: now_rfc3339(),
                    success: false,
                    aborted: false,
                    steps_attempted,
                    steps_completed,
                    error: Some(format!(
                        "failed to open I2C path {} for bus {}: {}",
                        i2c_path, bus, err
                    )),
                    step_results,
                    bash_commands: Vec::new(),
                },
                last_write_at,
            };
        }
    };

    for step in &steps {
        if abort_requested.load(Ordering::SeqCst) {
            aborted = true;
            break;
        }

        steps_attempted += 1;

        if let Some(last) = last_write_at {
            let elapsed = last.elapsed();
            if elapsed < min_spacing {
                std::thread::sleep(min_spacing - elapsed);
            }
        }

        let executed_at = now_rfc3339();
        match i2c.write(address, &[step.register, step.value]) {
            Ok(()) => {
                steps_completed += 1;
                step_results.push(LedLabStepResult {
                    register: step.register,
                    value: step.value,
                    delay_ms: step.delay_ms,
                    success: true,
                    error: None,
                    executed_at,
                });
                last_write_at = Some(Instant::now());

                let delay_ms = step.delay_ms.min(LED_LAB_MAX_STEP_DELAY_MS);
                if delay_ms > 0 {
                    std::thread::sleep(Duration::from_millis(delay_ms));
                }
            }
            Err(err) => {
                let err_text = format!(
                    "I2C write failed on /dev/i2c-{bus} addr 0x{address:02x} reg 0x{:02x} value 0x{:02x}: {}",
                    step.register, step.value, err
                );
                step_results.push(LedLabStepResult {
                    register: step.register,
                    value: step.value,
                    delay_ms: step.delay_ms,
                    success: false,
                    error: Some(err_text.clone()),
                    executed_at,
                });
                run_error = Some(err_text);
                break;
            }
        }
    }

    let finished_at = now_rfc3339();
    if aborted && run_error.is_none() {
        run_error = Some("run aborted by user".to_string());
    }

    let success = run_error.is_none() && !aborted && steps_completed == steps_attempted;

    BlockingRunOutput {
        result: LedLabRunResult {
            started_at,
            finished_at,
            success,
            aborted,
            steps_attempted,
            steps_completed,
            error: run_error,
            step_results,
            bash_commands: Vec::new(),
        },
        last_write_at,
    }
}

pub fn bash_preview_for_steps(bus: u8, device_address: u8, steps: &[LedLabStep]) -> Vec<String> {
    let mut lines = Vec::new();

    for step in steps {
        lines.push(format!(
            "i2cset -y {bus} 0x{device_address:02x} 0x{:02x} 0x{:02x}",
            step.register, step.value
        ));

        let delay = step.delay_ms.min(LED_LAB_MAX_STEP_DELAY_MS);
        if delay > 0 {
            lines.push(format!("sleep {:.3}", delay as f64 / 1000.0));
        }
    }

    lines
}

pub fn direct_single_color_steps(
    index: u8,
    red: u8,
    green: u8,
    blue: u8,
    delay_ms: u64,
) -> Vec<LedLabStep> {
    let delay = delay_ms.min(LED_LAB_MAX_STEP_DELAY_MS);
    vec![
        LedLabStep {
            register: 0x04,
            value: 0x00,
            delay_ms: 0,
            note: "enable direct mode".to_string(),
        },
        LedLabStep {
            register: 0x00,
            value: index,
            delay_ms: 0,
            note: "select LED index".to_string(),
        },
        LedLabStep {
            register: 0x01,
            value: red,
            delay_ms: 0,
            note: "set red".to_string(),
        },
        LedLabStep {
            register: 0x02,
            value: green,
            delay_ms: 0,
            note: "set green".to_string(),
        },
        LedLabStep {
            register: 0x03,
            value: blue,
            delay_ms: delay,
            note: "set blue".to_string(),
        },
    ]
}

pub fn validate_scan_bounds(
    register_start: u8,
    register_end: u8,
    value_start: u8,
    value_end: u8,
    allow_larger: bool,
) -> Result<usize> {
    ensure!(
        register_start <= register_end,
        "register start must be <= register end"
    );
    ensure!(value_start <= value_end, "value start must be <= value end");

    let register_count = usize::from(register_end - register_start) + 1;
    let value_count = usize::from(value_end - value_start) + 1;
    let total = register_count.saturating_mul(value_count);

    ensure!(total >= 1, "scan must include at least one command");
    ensure!(
        total <= LED_LAB_MAX_STEPS_PER_RUN,
        "scan cannot exceed {} commands",
        LED_LAB_MAX_STEPS_PER_RUN
    );

    if total > 8 {
        ensure!(
            allow_larger,
            "scan range includes {total} commands; set allow_larger=true to proceed"
        );
    }

    Ok(total)
}

pub fn build_scan_steps(
    register_start: u8,
    register_end: u8,
    value_start: u8,
    value_end: u8,
    delay_ms: u64,
) -> Vec<LedLabStep> {
    let mut steps = Vec::new();
    let normalized_delay = delay_ms.min(LED_LAB_MAX_STEP_DELAY_MS);

    for register in register_start..=register_end {
        for value in value_start..=value_end {
            steps.push(LedLabStep {
                register,
                value,
                delay_ms: normalized_delay,
                note: String::new(),
            });
        }
    }

    steps
}

fn validate_steps(steps: &[LedLabStep]) -> Result<()> {
    ensure!(!steps.is_empty(), "at least one step is required");
    ensure!(
        steps.len() <= LED_LAB_MAX_STEPS_PER_ENTRY,
        "step count cannot exceed {}",
        LED_LAB_MAX_STEPS_PER_ENTRY
    );

    for (idx, step) in steps.iter().enumerate() {
        ensure!(
            step.delay_ms <= LED_LAB_MAX_STEP_DELAY_MS,
            "steps[{idx}] delay exceeds {} ms",
            LED_LAB_MAX_STEP_DELAY_MS
        );
    }

    Ok(())
}

fn normalize_steps(mut steps: Vec<LedLabStep>) -> Vec<LedLabStep> {
    for step in &mut steps {
        step.delay_ms = step.delay_ms.min(LED_LAB_MAX_STEP_DELAY_MS);
        step.note = step.note.trim().to_string();
    }
    steps
}

fn immediate_failure(message: &str, steps: &[LedLabStep], bus: u8, address: u8) -> LedLabRunResult {
    let now = now_rfc3339();
    LedLabRunResult {
        started_at: now.clone(),
        finished_at: now,
        success: false,
        aborted: false,
        steps_attempted: 0,
        steps_completed: 0,
        error: Some(message.to_string()),
        step_results: Vec::new(),
        bash_commands: bash_preview_for_steps(bus, address, steps),
    }
}
