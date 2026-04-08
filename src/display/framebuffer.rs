use crate::display::render::PreviewFrame;
use core::convert::Infallible;
use embedded_graphics::{pixelcolor::BinaryColor, prelude::*, primitives::Rectangle};

pub struct FrameBufferTarget {
    pub frame: PreviewFrame,
}

impl FrameBufferTarget {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            frame: PreviewFrame::new(width, height),
        }
    }

    pub fn into_frame(self) -> PreviewFrame {
        self.frame
    }

    pub fn clear_buffer(&mut self) {
        self.frame.clear();
    }
}

impl OriginDimensions for FrameBufferTarget {
    fn size(&self) -> Size {
        Size::new(self.frame.width as u32, self.frame.height as u32)
    }
}

impl DrawTarget for FrameBufferTarget {
    type Color = BinaryColor;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            self.frame
                .set_pixel(point.x, point.y, matches!(color, BinaryColor::On));
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let on = matches!(color, BinaryColor::On);
        for y in 0..self.frame.height {
            for x in 0..self.frame.width {
                self.frame.set_pixel(x as i32, y as i32, on);
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let on = matches!(color, BinaryColor::On);

        let x0 = area.top_left.x.max(0);
        let y0 = area.top_left.y.max(0);
        let x1 = (area.top_left.x + area.size.width as i32).min(self.frame.width as i32);
        let y1 = (area.top_left.y + area.size.height as i32).min(self.frame.height as i32);

        for y in y0..y1 {
            for x in x0..x1 {
                self.frame.set_pixel(x, y, on);
            }
        }

        Ok(())
    }
}
