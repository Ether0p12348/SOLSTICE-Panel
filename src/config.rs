use crate::led::strip::{YAHBOOM_RGB_DEFAULT_ADDRESS, YAHBOOM_RGB_DEFAULT_LED_COUNT};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    pub display: DisplayConfig,
    pub web: WebConfig,
    #[serde(default)]
    pub led: LedConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub enabled: bool,
    pub width: u32,
    pub height: u32,
    pub i2c_path: String,
    pub address: u8,
    #[serde(default = "default_display_refresh_ms")]
    pub refresh_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebConfig {
    pub bind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedConfig {
    pub enabled: bool,
    pub i2c_path: String,
    pub address: u8,
    pub hardware_led_count: u16,
    #[serde(default = "default_fan_auto_on_temp_c")]
    pub fan_auto_on_temp_c: u8,
    #[serde(default = "default_led_effect_id")]
    pub default_effect_id: String,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            width: 128,
            height: 32,
            i2c_path: "/dev/i2c-7".to_string(),
            address: 60,
            refresh_ms: default_display_refresh_ms(),
        }
    }
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8080".to_string(),
        }
    }
}

impl Default for LedConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            i2c_path: "/dev/i2c-7".to_string(),
            address: YAHBOOM_RGB_DEFAULT_ADDRESS,
            hardware_led_count: YAHBOOM_RGB_DEFAULT_LED_COUNT,
            fan_auto_on_temp_c: default_fan_auto_on_temp_c(),
            default_effect_id: default_led_effect_id(),
        }
    }
}

fn default_fan_auto_on_temp_c() -> u8 {
    70
}

fn default_display_refresh_ms() -> u64 {
    100
}

fn default_led_effect_id() -> String {
    "effect-01".to_string()
}

fn parse_effect_id_mode(effect_id: &str) -> Option<u8> {
    let trimmed = effect_id.trim();
    let suffix = trimmed.strip_prefix("effect-")?;
    if suffix.len() != 2 || !suffix.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    u8::from_str_radix(suffix, 16).ok()
}

impl LedConfig {
    pub fn default_effect_mode(&self) -> u8 {
        parse_effect_id_mode(&self.default_effect_id).unwrap_or(1)
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            display: DisplayConfig::default(),
            web: WebConfig::default(),
            led: LedConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();

        let raw = fs::read_to_string(path_ref)
            .with_context(|| format!("failed to read config file: {}", path_ref.display()))?;

        let mut cfg: Self = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config file: {}", path_ref.display()))?;
        let raw_toml: toml::Value = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config file: {}", path_ref.display()))?;

        if let Some(refresh_ms) = raw_toml
            .get("runtime")
            .and_then(|section| section.get("refresh_ms"))
            .and_then(toml::Value::as_integer)
            .and_then(|value| u64::try_from(value).ok())
        {
            cfg.display.refresh_ms = refresh_ms;
        }

        let fan_auto_on_temp = raw_toml
            .get("fan")
            .and_then(|section| {
                section
                    .get("auto_on_temp_c")
                    .or_else(|| section.get("fan_auto_on_temp_c"))
            })
            .and_then(toml::Value::as_integer)
            .and_then(|value| u8::try_from(value).ok());
        if let Some(temp_c) = fan_auto_on_temp {
            cfg.led.fan_auto_on_temp_c = temp_c;
        }

        cfg.validate()?;
        Ok(cfg)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        self.validate()?;

        let path_ref = path.as_ref();
        let serialized = self.to_toml_pretty()?;

        fs::write(path_ref, serialized)
            .with_context(|| format!("failed to write config file: {}", path_ref.display()))?;

        Ok(())
    }

    pub fn to_toml_pretty(&self) -> Result<String> {
        #[derive(Serialize)]
        struct DisplayToml<'a> {
            enabled: bool,
            width: u32,
            height: u32,
            i2c_path: &'a str,
            address: u8,
        }
        #[derive(Serialize)]
        struct RuntimeToml {
            refresh_ms: u64,
        }
        #[derive(Serialize)]
        struct WebToml<'a> {
            bind: &'a str,
        }
        #[derive(Serialize)]
        struct LedToml<'a> {
            enabled: bool,
            default_effect_id: &'a str,
            i2c_path: &'a str,
            address: u8,
            hardware_led_count: u16,
        }
        #[derive(Serialize)]
        struct FanToml {
            auto_on_temp_c: u8,
        }

        let display = DisplayToml {
            enabled: self.display.enabled,
            width: self.display.width,
            height: self.display.height,
            i2c_path: &self.display.i2c_path,
            address: self.display.address,
        };
        let runtime = RuntimeToml {
            refresh_ms: self.display.refresh_ms,
        };
        let web = WebToml {
            bind: &self.web.bind,
        };
        let led = LedToml {
            enabled: self.led.enabled,
            default_effect_id: &self.led.default_effect_id,
            i2c_path: &self.led.i2c_path,
            address: self.led.address,
            hardware_led_count: self.led.hardware_led_count,
        };
        let fan = FanToml {
            auto_on_temp_c: self.led.fan_auto_on_temp_c,
        };

        let mut output = String::new();
        output.push_str("[display]\n");
        output.push_str(
            toml::to_string_pretty(&display)
                .context("failed to serialize [display] config section")?
                .trim_end(),
        );
        output.push_str("\n\n[runtime]\n");
        output.push_str(
            toml::to_string_pretty(&runtime)
                .context("failed to serialize [runtime] config section")?
                .trim_end(),
        );
        output.push_str("\n\n[web]\n");
        output.push_str(
            toml::to_string_pretty(&web)
                .context("failed to serialize [web] config section")?
                .trim_end(),
        );
        output.push_str("\n\n[led]\n");
        output.push_str(
            toml::to_string_pretty(&led)
                .context("failed to serialize [led] config section")?
                .trim_end(),
        );
        output.push_str("\n\n[fan]\n");
        output.push_str(
            toml::to_string_pretty(&fan)
                .context("failed to serialize [fan] config section")?
                .trim_end(),
        );
        output.push('\n');

        Ok(output)
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.display.width == 128,
            "only 128px width is supported right now"
        );
        anyhow::ensure!(
            self.display.height == 32,
            "only 32px height is supported right now"
        );
        anyhow::ensure!(
            self.display.refresh_ms >= 100,
            "display.refresh_ms must be at least 100"
        );

        anyhow::ensure!(
            !self.display.i2c_path.trim().is_empty(),
            "display.i2c_path cannot be empty"
        );
        anyhow::ensure!(!self.web.bind.trim().is_empty(), "web.bind cannot be empty");
        anyhow::ensure!(
            !self.led.i2c_path.trim().is_empty(),
            "led.i2c_path cannot be empty"
        );
        anyhow::ensure!(
            self.led.address <= 0x7f,
            "led.address must be a 7-bit value (0-127)"
        );
        anyhow::ensure!(
            self.led.hardware_led_count >= 1,
            "led.hardware_led_count must be at least 1"
        );
        anyhow::ensure!(
            self.led.hardware_led_count <= 255,
            "led.hardware_led_count must be at most 255"
        );
        anyhow::ensure!(
            (30..=110).contains(&self.led.fan_auto_on_temp_c),
            "led.fan_auto_on_temp_c must be between 30 and 110"
        );
        let default_effect_mode = self.led.default_effect_mode();
        anyhow::ensure!(
            default_effect_mode <= 6,
            "led.default_effect_id must be one of effect-00 through effect-06"
        );

        Ok(())
    }
}
