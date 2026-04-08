use crate::{config::DisplayConfig, display::render::PreviewFrame};
use anyhow::{Context, Result, anyhow};
use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};
use linux_embedded_hal::I2cdev;
use ssd1306::{
    I2CDisplayInterface, Ssd1306,
    mode::{BufferedGraphicsMode, DisplayConfig as Ssd1306DisplayConfig},
    prelude::{DisplayRotation, DisplaySize128x32, I2CInterface},
};

type OledDisplay =
    Ssd1306<I2CInterface<I2cdev>, DisplaySize128x32, BufferedGraphicsMode<DisplaySize128x32>>;

pub struct OledPanel {
    display: OledDisplay,
}

impl OledPanel {
    pub fn new(cfg: &DisplayConfig) -> Result<Self> {
        let i2c = I2cdev::new(&cfg.i2c_path)
            .with_context(|| format!("failed to open I2C device {}", cfg.i2c_path))?;

        let interface = I2CDisplayInterface::new_custom_address(i2c, cfg.address);
        let mut display = Ssd1306::new(interface, DisplaySize128x32, DisplayRotation::Rotate0)
            .into_buffered_graphics_mode();

        display
            .init()
            .map_err(|e| anyhow!("failed to initialize SSD1306: {:?}", e))?;

        display
            .flush()
            .map_err(|e| anyhow!("failed to flush SSD1306 after init: {:?}", e))?;

        Ok(Self { display })
    }

    pub fn clear(&mut self) -> Result<()> {
        self.display
            .clear(BinaryColor::Off)
            .map_err(|e| anyhow!("failed to clear display: {:?}", e))?;
        self.display
            .flush()
            .map_err(|e| anyhow!("failed to flush cleared display: {:?}", e))?;
        Ok(())
    }

    pub fn show_frame(&mut self, frame: &PreviewFrame) -> Result<()> {
        self.display
            .clear(BinaryColor::Off)
            .map_err(|e| anyhow!("failed to clear display: {:?}", e))?;

        for y in 0..frame.height {
            for x in 0..frame.width {
                let idx = y * frame.width + x;
                if frame.pixels.get(idx).copied().unwrap_or(0) == 1 {
                    Pixel(Point::new(x as i32, y as i32), BinaryColor::On)
                        .draw(&mut self.display)
                        .map_err(|e| anyhow!("failed to draw pixel: {:?}", e))?;
                }
            }
        }

        self.display
            .flush()
            .map_err(|e| anyhow!("failed to flush custom frame: {:?}", e))?;

        Ok(())
    }
}
