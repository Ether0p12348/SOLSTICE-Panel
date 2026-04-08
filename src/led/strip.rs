use anyhow::{Context, Result, anyhow, ensure};
use embedded_hal::i2c::I2c;
use linux_embedded_hal::I2cdev;
use std::time::Duration;

use crate::led::{model::LedColor, offload::BuiltinLedProgram};

pub const YAHBOOM_RGB_DEFAULT_ADDRESS: u8 = 0x0e;
pub const YAHBOOM_RGB_DEFAULT_LED_COUNT: u16 = 14;

const REG_LED_INDEX: u8 = 0x00;
const REG_RED: u8 = 0x01;
const REG_GREEN: u8 = 0x02;
const REG_BLUE: u8 = 0x03;
const REG_EFFECT_MODE: u8 = 0x04;
const REG_EFFECT_SPEED: u8 = 0x05;
const REG_EFFECT_COLOR: u8 = 0x06;
const REG_FAN: u8 = 0x08;
const EFFECT_MODE_MANUAL: u8 = 0x00;
const LED_INDEX_ALL: u8 = 0xff;
const FAN_OFF: u8 = 0x00;
const FAN_ON: u8 = 0x01;
const REGISTER_WRITE_RETRIES: usize = 3;
const REGISTER_WRITE_RETRY_DELAY_US: u64 = 800;
const REGISTER_WRITE_SPACING_US: u64 = 200;
const MANUAL_MODE_SETTLE_MS: u64 = 2;

pub struct YahboomLedStrip {
    i2c: I2cdev,
    i2c_path: String,
    address: u8,
    led_count: u16,
    last_frame: Vec<LedColor>,
    manual_mode_set: bool,
    builtin_program: Option<CachedBuiltinProgram>,
    fan_state: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CachedBuiltinProgram {
    mode: u8,
    speed: u8,
    color_index: u8,
}

impl YahboomLedStrip {
    pub fn new(i2c_path: &str, address: u8, led_count: u16) -> Result<Self> {
        ensure!(!i2c_path.trim().is_empty(), "LED I2C path cannot be empty");
        ensure!(address <= 0x7f, "LED I2C address must be 7-bit (0-127)");
        ensure!(led_count >= 1, "LED count must be at least 1");
        ensure!(led_count <= 255, "LED count must be at most 255");

        let i2c = I2cdev::new(i2c_path)
            .with_context(|| format!("failed to open LED I2C device {}", i2c_path))?;

        Ok(Self {
            i2c,
            i2c_path: i2c_path.to_string(),
            address,
            led_count,
            last_frame: vec![LedColor::rgb(0, 0, 0); led_count as usize],
            manual_mode_set: false,
            builtin_program: None,
            fan_state: None,
        })
    }

    pub fn clear(&mut self) -> Result<()> {
        self.disable_builtin_effect()?;
        self.write_register(REG_LED_INDEX, LED_INDEX_ALL)?;
        self.write_register(REG_RED, 0)?;
        self.write_register(REG_GREEN, 0)?;
        self.write_register(REG_BLUE, 0)?;
        self.last_frame.fill(LedColor::rgb(0, 0, 0));
        Ok(())
    }

    pub fn set_manual_mode(&mut self) -> Result<()> {
        if self.manual_mode_set {
            return Ok(());
        }
        self.write_register(REG_EFFECT_MODE, EFFECT_MODE_MANUAL)?;
        self.manual_mode_set = true;
        self.builtin_program = None;
        std::thread::sleep(Duration::from_millis(MANUAL_MODE_SETTLE_MS));
        Ok(())
    }

    pub fn disable_builtin_effect(&mut self) -> Result<()> {
        self.set_manual_mode()
    }

    pub fn set_builtin_effect(&mut self, program: &BuiltinLedProgram) -> Result<bool> {
        ensure!(
            (1..=6).contains(&program.mode),
            "built-in effect mode must be 1..=6"
        );
        ensure!(
            (1..=3).contains(&program.speed),
            "built-in effect speed must be 1..=3"
        );
        ensure!(
            program.color_index <= 6,
            "built-in effect color index must be 0..=6"
        );

        let desired = CachedBuiltinProgram {
            mode: program.mode,
            speed: program.speed,
            color_index: program.color_index,
        };
        if self.builtin_program.as_ref() == Some(&desired) {
            return Ok(false);
        }

        let previous = self.builtin_program;
        if previous.map(|p| p.mode) != Some(desired.mode) {
            self.write_register(REG_EFFECT_MODE, desired.mode)?;
        }
        if previous.map(|p| p.speed) != Some(desired.speed) {
            self.write_register(REG_EFFECT_SPEED, desired.speed)?;
        }
        if previous.map(|p| p.color_index) != Some(desired.color_index) {
            self.write_register(REG_EFFECT_COLOR, desired.color_index)?;
        }

        self.manual_mode_set = false;
        self.builtin_program = Some(desired);
        Ok(true)
    }

    pub fn snapshot_frame(&self) -> Vec<LedColor> {
        self.last_frame.clone()
    }

    pub fn write_direct_pixel(&mut self, index: u8, color: LedColor) -> Result<()> {
        self.set_manual_mode()?;
        self.set_single_led(index, color)?;
        if (index as usize) < self.last_frame.len() {
            self.last_frame[index as usize] = color;
        }
        Ok(())
    }

    pub fn set_fan_enabled(&mut self, enabled: bool) -> Result<bool> {
        if self.fan_state == Some(enabled) {
            return Ok(false);
        }

        let value = if enabled { FAN_ON } else { FAN_OFF };
        self.write_register(REG_FAN, value)?;
        self.fan_state = Some(enabled);
        Ok(true)
    }

    fn set_single_led(&mut self, index: u8, color: LedColor) -> Result<()> {
        if index as u16 >= self.led_count {
            return Err(anyhow!(
                "LED index {} is out of range for strip length {}",
                index,
                self.led_count
            ));
        }
        self.write_register(REG_LED_INDEX, index)?;
        self.write_register(REG_RED, color.r)?;
        self.write_register(REG_GREEN, color.g)?;
        self.write_register(REG_BLUE, color.b)?;
        Ok(())
    }

    fn write_register(&mut self, register: u8, value: u8) -> Result<()> {
        let mut last_err = None;
        for attempt in 1..=REGISTER_WRITE_RETRIES {
            match self.i2c.write(self.address, &[register, value]) {
                Ok(()) => {
                    std::thread::sleep(Duration::from_micros(REGISTER_WRITE_SPACING_US));
                    return Ok(());
                }
                Err(err) => {
                    last_err = Some(err);
                    if attempt < REGISTER_WRITE_RETRIES {
                        std::thread::sleep(Duration::from_micros(REGISTER_WRITE_RETRY_DELAY_US));
                    }
                }
            }
        }
        Err(anyhow!(
            "LED I2C write failed after {REGISTER_WRITE_RETRIES} attempts on {} addr 0x{:02X} (reg 0x{register:02X}, value {value}): {:?}",
            self.i2c_path,
            self.address,
            last_err
        ))
    }
}
