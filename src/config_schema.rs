use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

use crate::config::AppConfig;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFieldType {
    Text,
    Integer,
    Boolean,
    Select,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigFieldOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigField {
    pub key: String,
    pub label: String,
    pub description: String,
    pub field_type: ConfigFieldType,
    pub value: Value,
    pub default_value: Option<Value>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub advanced: bool,
    pub read_only: bool,
    pub placeholder: Option<String>,
    pub options: Vec<ConfigFieldOption>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigSection {
    pub id: String,
    pub label: String,
    pub description: String,
    pub fields: Vec<ConfigField>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigSchema {
    pub sections: Vec<ConfigSection>,
}

#[derive(Debug, Deserialize)]
pub struct ConfigGuiUpdateRequest {
    pub values: HashMap<String, Value>,
}

#[derive(Debug, Clone, Copy)]
enum SectionId {
    Display,
    Runtime,
    Web,
    Led,
    Fan,
}

impl SectionId {
    const ALL: [SectionId; 5] = [
        SectionId::Display,
        SectionId::Runtime,
        SectionId::Web,
        SectionId::Led,
        SectionId::Fan,
    ];

    fn id(self) -> &'static str {
        match self {
            SectionId::Display => "display",
            SectionId::Runtime => "runtime",
            SectionId::Web => "web",
            SectionId::Led => "led",
            SectionId::Fan => "fan",
        }
    }

    fn label(self) -> &'static str {
        match self {
            SectionId::Display => "Display",
            SectionId::Runtime => "Runtime / Refresh",
            SectionId::Web => "Web",
            SectionId::Led => "LED Studio",
            SectionId::Fan => "Fan",
        }
    }

    fn description(self) -> &'static str {
        match self {
            SectionId::Display => "Hardware and panel transport settings.",
            SectionId::Runtime => "Display behavior controls.",
            SectionId::Web => "HTTP bind settings for dashboard and Studio.",
            SectionId::Led => "LED runtime and hardware controls.",
            SectionId::Fan => "Cooling safety controls tied to standby and host temperature.",
        }
    }
}

type ConfigFieldOptionSpec = (&'static str, &'static str);

const LED_DEFAULT_EFFECT_OPTIONS: &[ConfigFieldOptionSpec] = &[
    ("effect-00", "OFF / Direct"),
    ("effect-01", "Breathing Effect"),
    ("effect-02", "Single Random Cycle"),
    ("effect-03", "Smooth Rainbow"),
    ("effect-04", "Alternating Cycle"),
    ("effect-05", "Single Cycle"),
    ("effect-06", "Breathing Rainbow"),
];

struct ConfigFieldSpec {
    key: &'static str,
    section: SectionId,
    label: &'static str,
    description: &'static str,
    field_type: ConfigFieldType,
    advanced: bool,
    read_only: bool,
    placeholder: Option<&'static str>,
    min: Option<i64>,
    max: Option<i64>,
    options: &'static [ConfigFieldOptionSpec],
    getter: fn(&AppConfig) -> Value,
    setter: fn(&mut AppConfig, &Value) -> Result<()>,
}

pub fn build_config_schema(config: &AppConfig) -> ConfigSchema {
    let defaults = AppConfig::default();

    let mut sections: Vec<ConfigSection> = SectionId::ALL
        .into_iter()
        .map(|section| ConfigSection {
            id: section.id().to_string(),
            label: section.label().to_string(),
            description: section.description().to_string(),
            fields: Vec::new(),
        })
        .collect();

    for spec in config_field_specs() {
        let field = ConfigField {
            key: spec.key.to_string(),
            label: spec.label.to_string(),
            description: spec.description.to_string(),
            field_type: spec.field_type,
            value: (spec.getter)(config),
            default_value: Some((spec.getter)(&defaults)),
            min: spec.min,
            max: spec.max,
            advanced: spec.advanced,
            read_only: spec.read_only,
            placeholder: spec.placeholder.map(str::to_string),
            options: spec
                .options
                .iter()
                .map(|(value, label)| ConfigFieldOption {
                    value: (*value).to_string(),
                    label: (*label).to_string(),
                })
                .collect(),
        };

        if let Some(section) = sections.iter_mut().find(|s| s.id == spec.section.id()) {
            section.fields.push(field);
        }
    }

    ConfigSchema { sections }
}

pub fn apply_config_gui_values(
    config: &mut AppConfig,
    values: &HashMap<String, Value>,
) -> Result<()> {
    for key in values.keys() {
        ensure!(
            find_field_spec(key).is_some(),
            "unknown config field '{key}'"
        );
    }

    for spec in config_field_specs() {
        if spec.read_only {
            continue;
        }

        if let Some(value) = values.get(spec.key) {
            (spec.setter)(config, value)
                .with_context(|| format!("failed updating '{}': invalid value", spec.key))?;
        }
    }

    config.validate()
}

fn find_field_spec(key: &str) -> Option<&'static ConfigFieldSpec> {
    config_field_specs().iter().find(|spec| spec.key == key)
}

fn config_field_specs() -> &'static [ConfigFieldSpec] {
    &[
        ConfigFieldSpec {
            key: "display.enabled",
            section: SectionId::Display,
            label: "Display Enabled",
            description: "Enable or disable writing to the physical OLED panel.",
            field_type: ConfigFieldType::Boolean,
            advanced: false,
            read_only: false,
            placeholder: None,
            min: None,
            max: None,
            options: &[],
            getter: |cfg| json!(cfg.display.enabled),
            setter: |cfg, value| {
                cfg.display.enabled = expect_bool(value, "display.enabled")?;
                Ok(())
            },
        },
        ConfigFieldSpec {
            key: "display.width",
            section: SectionId::Display,
            label: "OLED Width",
            description: "Panel width in pixels. Current runtime supports 128.",
            field_type: ConfigFieldType::Integer,
            advanced: false,
            read_only: false,
            placeholder: None,
            min: Some(1),
            max: Some(1024),
            options: &[],
            getter: |cfg| json!(cfg.display.width),
            setter: |cfg, value| {
                cfg.display.width = expect_u32(value, "display.width")?;
                Ok(())
            },
        },
        ConfigFieldSpec {
            key: "display.height",
            section: SectionId::Display,
            label: "OLED Height",
            description: "Panel height in pixels. Current runtime supports 32.",
            field_type: ConfigFieldType::Integer,
            advanced: false,
            read_only: false,
            placeholder: None,
            min: Some(1),
            max: Some(512),
            options: &[],
            getter: |cfg| json!(cfg.display.height),
            setter: |cfg, value| {
                cfg.display.height = expect_u32(value, "display.height")?;
                Ok(())
            },
        },
        ConfigFieldSpec {
            key: "display.i2c_path",
            section: SectionId::Display,
            label: "I2C Path",
            description: "Linux device path for OLED I2C bus.",
            field_type: ConfigFieldType::Text,
            advanced: true,
            read_only: false,
            placeholder: Some("/dev/i2c-7"),
            min: None,
            max: None,
            options: &[],
            getter: |cfg| json!(cfg.display.i2c_path),
            setter: |cfg, value| {
                cfg.display.i2c_path = expect_string(value, "display.i2c_path")?;
                Ok(())
            },
        },
        ConfigFieldSpec {
            key: "display.address",
            section: SectionId::Display,
            label: "I2C Address",
            description: "OLED address (7-bit, decimal).",
            field_type: ConfigFieldType::Integer,
            advanced: true,
            read_only: false,
            placeholder: None,
            min: Some(0),
            max: Some(127),
            options: &[],
            getter: |cfg| json!(cfg.display.address),
            setter: |cfg, value| {
                cfg.display.address = expect_u8(value, "display.address")?;
                Ok(())
            },
        },
        ConfigFieldSpec {
            key: "runtime.refresh_ms",
            section: SectionId::Runtime,
            label: "Refresh Interval (ms)",
            description: "How often metrics are collected and frames are rendered.",
            field_type: ConfigFieldType::Integer,
            advanced: false,
            read_only: false,
            placeholder: None,
            min: Some(100),
            max: Some(60000),
            options: &[],
            getter: |cfg| json!(cfg.display.refresh_ms),
            setter: |cfg, value| {
                cfg.display.refresh_ms = expect_u64(value, "runtime.refresh_ms")?;
                Ok(())
            },
        },
        ConfigFieldSpec {
            key: "web.bind",
            section: SectionId::Web,
            label: "* Web Bind Address",
            description: "Bind host:port for dashboard and Studio web server.",
            field_type: ConfigFieldType::Text,
            advanced: false,
            read_only: false,
            placeholder: Some("0.0.0.0:8080"),
            min: None,
            max: None,
            options: &[],
            getter: |cfg| json!(cfg.web.bind),
            setter: |cfg, value| {
                cfg.web.bind = expect_string(value, "web.bind")?;
                Ok(())
            },
        },
        ConfigFieldSpec {
            key: "led.enabled",
            section: SectionId::Led,
            label: "LED Studio Enabled",
            description: "Enable LED Studio editing workflows and persistence.",
            field_type: ConfigFieldType::Boolean,
            advanced: false,
            read_only: false,
            placeholder: None,
            min: None,
            max: None,
            options: &[],
            getter: |cfg| json!(cfg.led.enabled),
            setter: |cfg, value| {
                cfg.led.enabled = expect_bool(value, "led.enabled")?;
                Ok(())
            },
        },
        ConfigFieldSpec {
            key: "led.i2c_path",
            section: SectionId::Led,
            label: "LED I2C Path",
            description: "Linux I2C device path used by LED runtime output.",
            field_type: ConfigFieldType::Text,
            advanced: true,
            read_only: false,
            placeholder: Some("/dev/i2c-7"),
            min: None,
            max: None,
            options: &[],
            getter: |cfg| json!(cfg.led.i2c_path),
            setter: |cfg, value| {
                cfg.led.i2c_path = expect_string(value, "led.i2c_path")?;
                Ok(())
            },
        },
        ConfigFieldSpec {
            key: "led.address",
            section: SectionId::Led,
            label: "LED I2C Address",
            description: "Yahboom RGB controller address (7-bit, decimal).",
            field_type: ConfigFieldType::Integer,
            advanced: true,
            read_only: false,
            placeholder: Some("14"),
            min: Some(0),
            max: Some(127),
            options: &[],
            getter: |cfg| json!(cfg.led.address),
            setter: |cfg, value| {
                cfg.led.address = expect_u8(value, "led.address")?;
                Ok(())
            },
        },
        ConfigFieldSpec {
            key: "led.hardware_led_count",
            section: SectionId::Led,
            label: "Hardware LED Count",
            description: "Number of physical LEDs addressable by runtime hardware.",
            field_type: ConfigFieldType::Integer,
            advanced: true,
            read_only: false,
            placeholder: Some("14"),
            min: Some(1),
            max: Some(255),
            options: &[],
            getter: |cfg| json!(cfg.led.hardware_led_count),
            setter: |cfg, value| {
                cfg.led.hardware_led_count = expect_u16(value, "led.hardware_led_count")?;
                Ok(())
            },
        },
        ConfigFieldSpec {
            key: "led.default_effect_id",
            section: SectionId::Led,
            label: "Default LED Effect",
            description: "Default built-in LED effect selected at startup for controller mode (effect-00..effect-06).",
            field_type: ConfigFieldType::Select,
            advanced: false,
            read_only: false,
            placeholder: None,
            min: None,
            max: None,
            options: LED_DEFAULT_EFFECT_OPTIONS,
            getter: |cfg| json!(cfg.led.default_effect_id),
            setter: |cfg, value| {
                cfg.led.default_effect_id = expect_string(value, "led.default_effect_id")?;
                Ok(())
            },
        },
        ConfigFieldSpec {
            key: "fan.auto_on_temp_c",
            section: SectionId::Fan,
            label: "Fan Auto-On Temp (C)",
            description: "CPU temperature threshold used to force fan on while standby mode is active.",
            field_type: ConfigFieldType::Integer,
            advanced: false,
            read_only: false,
            placeholder: Some("70"),
            min: Some(30),
            max: Some(110),
            options: &[],
            getter: |cfg| json!(cfg.led.fan_auto_on_temp_c),
            setter: |cfg, value| {
                cfg.led.fan_auto_on_temp_c = expect_u8(value, "fan.auto_on_temp_c")?;
                Ok(())
            },
        },
    ]
}

fn expect_bool(value: &Value, key: &str) -> Result<bool> {
    value
        .as_bool()
        .with_context(|| format!("{key} must be a boolean"))
}

fn expect_string(value: &Value, key: &str) -> Result<String> {
    value
        .as_str()
        .map(str::to_string)
        .with_context(|| format!("{key} must be a string"))
}

fn expect_i64(value: &Value, key: &str) -> Result<i64> {
    if let Some(number) = value.as_i64() {
        return Ok(number);
    }

    if let Some(number) = value.as_u64() {
        return i64::try_from(number).with_context(|| format!("{key} is out of range"));
    }

    if let Some(text) = value.as_str() {
        return text
            .parse::<i64>()
            .with_context(|| format!("{key} must be an integer"));
    }

    Err(anyhow::anyhow!("{key} must be an integer"))
}

fn expect_u8(value: &Value, key: &str) -> Result<u8> {
    let parsed = expect_i64(value, key)?;
    ensure!(parsed >= 0, "{key} must be non-negative");
    u8::try_from(parsed).with_context(|| format!("{key} is out of range"))
}

fn expect_u32(value: &Value, key: &str) -> Result<u32> {
    let parsed = expect_i64(value, key)?;
    ensure!(parsed >= 0, "{key} must be non-negative");
    u32::try_from(parsed).with_context(|| format!("{key} is out of range"))
}

fn expect_u16(value: &Value, key: &str) -> Result<u16> {
    let parsed = expect_i64(value, key)?;
    ensure!(parsed >= 0, "{key} must be non-negative");
    u16::try_from(parsed).with_context(|| format!("{key} is out of range"))
}

fn expect_u64(value: &Value, key: &str) -> Result<u64> {
    let parsed = expect_i64(value, key)?;
    ensure!(parsed >= 0, "{key} must be non-negative");
    u64::try_from(parsed).with_context(|| format!("{key} is out of range"))
}
