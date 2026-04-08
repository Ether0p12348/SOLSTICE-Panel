use crate::{
    display::image_element::load_monochrome_mask,
    metrics::SystemMetrics,
    page_store::{
        DynamicSource, MonoColor, PageDefinition, PageElement, TEXT_HEIGHT_MIN_PX, TextSize,
    },
};
use chrono::{Local, SecondsFormat};
use embedded_graphics::{
    mono_font::{
        MonoFont, MonoTextStyle,
        ascii::{FONT_4X6, FONT_6X10},
    },
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyleBuilder, Rectangle},
    text::{Baseline, Text},
};
use std::{
    convert::Infallible,
    time::{SystemTime, UNIX_EPOCH},
};

fn truncate(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        input.to_string()
    } else {
        input.chars().take(max_chars).collect()
    }
}

pub struct DynamicRenderContext<'a> {
    pub active_page_label: &'a str,
    pub display_mode_label: &'a str,
    pub rotation_active: bool,
    pub rotation_interval_ms: u64,
    pub rotation_queue_len: usize,
    pub rotation_index: Option<usize>,
    pub display_width: u32,
    pub display_height: u32,
    pub refresh_ms: u64,
    pub i2c_address: u8,
    pub web_bind: &'a str,
}

fn resolve_dynamic_text(
    source: &DynamicSource,
    page: &PageDefinition,
    metrics: &SystemMetrics,
    context: &DynamicRenderContext<'_>,
) -> String {
    match source {
        DynamicSource::Hostname => metrics.hostname.clone(),
        DynamicSource::IpAddr => metrics.ip_addr.clone(),
        DynamicSource::LocalTimeHm => Local::now().format("%H:%M").to_string(),
        DynamicSource::LocalTimeHms => Local::now().format("%H:%M:%S").to_string(),
        DynamicSource::LocalDateYmd => Local::now().format("%Y-%m-%d").to_string(),
        DynamicSource::LocalDateDmy => Local::now().format("%d/%m/%Y").to_string(),
        DynamicSource::LocalDateMdy => Local::now().format("%m/%d/%Y").to_string(),
        DynamicSource::LocalDateTimeIso => Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        DynamicSource::LocalDateTimeCompact => Local::now().format("%Y%m%d-%H%M%S").to_string(),
        DynamicSource::LocalDateTimeRfc2822 => Local::now().to_rfc2822(),
        DynamicSource::LocalDateTimeRfc3339 => {
            Local::now().to_rfc3339_opts(SecondsFormat::Secs, false)
        }
        DynamicSource::UptimeText => metrics.uptime_text.clone(),
        DynamicSource::RamPercentText => metrics.ram_percent_text.clone(),
        DynamicSource::CpuTempText => metrics.cpu_temp_text.clone(),
        DynamicSource::CpuUsagePercentText => metrics.cpu_usage_percent_text.clone(),
        DynamicSource::LoadAvg1 => metrics.load_avg_1.clone(),
        DynamicSource::LoadAvg5 => metrics.load_avg_5.clone(),
        DynamicSource::LoadAvg15 => metrics.load_avg_15.clone(),
        DynamicSource::LoadAvgText => metrics.load_avg_text.clone(),
        DynamicSource::MemTotalMibText => metrics.mem_total_mib_text.clone(),
        DynamicSource::MemUsedMibText => metrics.mem_used_mib_text.clone(),
        DynamicSource::MemAvailableMibText => metrics.mem_available_mib_text.clone(),
        DynamicSource::MemFreeMibText => metrics.mem_free_mib_text.clone(),
        DynamicSource::SwapTotalMibText => metrics.swap_total_mib_text.clone(),
        DynamicSource::SwapUsedMibText => metrics.swap_used_mib_text.clone(),
        DynamicSource::SwapFreeMibText => metrics.swap_free_mib_text.clone(),
        DynamicSource::SwapUsedPercentText => metrics.swap_used_percent_text.clone(),
        DynamicSource::ProcsRunning => metrics.procs_running_text.clone(),
        DynamicSource::ProcsBlocked => metrics.procs_blocked_text.clone(),
        DynamicSource::CpuCores => metrics.cpu_cores_text.clone(),
        DynamicSource::OsPrettyName => metrics.os_pretty_name.clone(),
        DynamicSource::KernelRelease => metrics.kernel_release.clone(),
        DynamicSource::ActivePage => context.active_page_label.to_string(),
        DynamicSource::ActivePageId => context.active_page_label.to_string(),
        DynamicSource::DisplayMode => context.display_mode_label.to_string(),
        DynamicSource::RotationActive => {
            if context.rotation_active {
                "active".to_string()
            } else {
                "paused".to_string()
            }
        }
        DynamicSource::RotationIntervalMs => context.rotation_interval_ms.to_string(),
        DynamicSource::RotationIntervalSeconds => (context.rotation_interval_ms / 1000).to_string(),
        DynamicSource::RotationQueueLen => context.rotation_queue_len.to_string(),
        DynamicSource::RotationQueueEmpty => {
            if context.rotation_queue_len == 0 {
                "yes".to_string()
            } else {
                "no".to_string()
            }
        }
        DynamicSource::RotationIndex => context
            .rotation_index
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "-".to_string()),
        DynamicSource::RotationNextIndex => {
            if context.rotation_queue_len == 0 {
                "-".to_string()
            } else {
                let next = context
                    .rotation_index
                    .map(|idx| (idx + 1) % context.rotation_queue_len)
                    .unwrap_or(0);
                next.to_string()
            }
        }
        DynamicSource::RotationPosition => context
            .rotation_index
            .map(|idx| format!("{}/{}", idx + 1, context.rotation_queue_len))
            .unwrap_or_else(|| "-".to_string()),
        DynamicSource::DisplayWidth => context.display_width.to_string(),
        DynamicSource::DisplayHeight => context.display_height.to_string(),
        DynamicSource::RefreshMs => context.refresh_ms.to_string(),
        DynamicSource::I2cAddress => format!("{}", context.i2c_address),
        DynamicSource::I2cAddressHex => format!("0x{:02X}", context.i2c_address),
        DynamicSource::WebBind => context.web_bind.to_string(),
        DynamicSource::PageId => page.id.clone(),
        DynamicSource::PageName => page.name.clone(),
        DynamicSource::PageVersion => page.meta.version.clone(),
        DynamicSource::PageAuthors => {
            if page.meta.authors.is_empty() {
                "-".to_string()
            } else {
                page.meta.authors.join(", ")
            }
        }
        DynamicSource::PageAuthorCount => page.meta.authors.len().to_string(),
        DynamicSource::PageBundle => page
            .meta
            .bundle_name
            .clone()
            .unwrap_or_else(|| "-".to_string()),
        DynamicSource::PageLicense => page.meta.license.clone().unwrap_or_else(|| "-".to_string()),
        DynamicSource::PageSourceUrl => page
            .meta
            .source_url
            .clone()
            .unwrap_or_else(|| "-".to_string()),
        DynamicSource::PageTags => {
            if page.meta.tags.is_empty() {
                "-".to_string()
            } else {
                page.meta.tags.join(",")
            }
        }
        DynamicSource::PageTagCount => page.meta.tags.len().to_string(),
        DynamicSource::PageDescription => page
            .meta
            .description
            .clone()
            .unwrap_or_else(|| "-".to_string()),
        DynamicSource::PageElementCount => page.elements.len().to_string(),
        DynamicSource::PageWidth => page.width.to_string(),
        DynamicSource::PageHeight => page.height.to_string(),
        DynamicSource::UnixEpochSeconds => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string()),
        DynamicSource::UnixEpochMillis => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_else(|_| "0".to_string()),
    }
}

fn binary(color: &MonoColor) -> BinaryColor {
    match color {
        MonoColor::On => BinaryColor::On,
        MonoColor::Off => BinaryColor::Off,
    }
}

fn default_text_height_for_size(size: &TextSize) -> u32 {
    match size {
        TextSize::Small => 6,
        TextSize::Large => 10,
    }
}

fn effective_text_height_px(size: &TextSize, text_height_px: u32) -> u32 {
    if text_height_px == 0 {
        default_text_height_for_size(size)
    } else {
        text_height_px.max(TEXT_HEIGHT_MIN_PX)
    }
}

fn scale_dim(src: u32, src_height: u32, dst_height: u32) -> u32 {
    if src_height == 0 {
        return src.max(1);
    }
    let scaled = (src.saturating_mul(dst_height) + (src_height / 2)) / src_height;
    scaled.max(1)
}

fn base_font_for_height(text_height_px: u32) -> &'static MonoFont<'static> {
    if text_height_px >= 8 {
        &FONT_6X10
    } else {
        &FONT_4X6
    }
}

struct GlyphMask {
    width: usize,
    height: usize,
    pixels: Vec<bool>,
}

impl GlyphMask {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![false; width.saturating_mul(height)],
        }
    }

    fn set_pixel(&mut self, x: i32, y: i32, on: bool) {
        if x < 0 || y < 0 {
            return;
        }
        let x = x as usize;
        let y = y as usize;
        if x >= self.width || y >= self.height {
            return;
        }
        self.pixels[y * self.width + x] = on;
    }

    fn is_on(&self, x: usize, y: usize) -> bool {
        self.pixels[y * self.width + x]
    }
}

impl OriginDimensions for GlyphMask {
    fn size(&self) -> Size {
        Size::new(self.width as u32, self.height as u32)
    }
}

impl DrawTarget for GlyphMask {
    type Color = BinaryColor;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            self.set_pixel(point.x, point.y, color == BinaryColor::On);
        }
        Ok(())
    }
}

fn draw_scaled_text<D>(
    display: &mut D,
    text: &str,
    x: i32,
    baseline_y: i32,
    text_height_px: u32,
    color: &MonoColor,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let font = base_font_for_height(text_height_px);
    let src_w = font.character_size.width.max(1);
    let src_h = font.character_size.height.max(1);
    let dst_h = text_height_px.max(TEXT_HEIGHT_MIN_PX);
    let dst_w = scale_dim(src_w, src_h, dst_h);
    let dst_spacing = if font.character_spacing == 0 {
        0
    } else {
        scale_dim(font.character_spacing, src_h, dst_h)
    };

    let top_y = baseline_y - (dst_h as i32 - 2);
    let draw_color = binary(color);

    let mut cursor_x = x;
    for ch in text.chars() {
        let mut mask = GlyphMask::new(src_w as usize, src_h as usize);
        let glyph_style = MonoTextStyle::new(font, BinaryColor::On);
        let glyph_text = ch.to_string();
        let _ = Text::with_baseline(&glyph_text, Point::zero(), glyph_style, Baseline::Top)
            .draw(&mut mask);

        let mut pixels: Vec<Pixel<BinaryColor>> = Vec::new();
        for out_y in 0..dst_h {
            let src_y = ((out_y as usize) * src_h as usize) / dst_h as usize;
            for out_x in 0..dst_w {
                let src_x = ((out_x as usize) * src_w as usize) / dst_w as usize;
                if mask.is_on(src_x, src_y) {
                    pixels.push(Pixel(
                        Point::new(cursor_x + out_x as i32, top_y + out_y as i32),
                        draw_color,
                    ));
                }
            }
        }
        display.draw_iter(pixels.into_iter())?;
        cursor_x += dst_w as i32 + dst_spacing as i32;
    }

    Ok(())
}

fn draw_mask_image<D>(
    display: &mut D,
    x: i32,
    y: i32,
    foreground: &MonoColor,
    background: Option<&MonoColor>,
    mask: &[bool],
    width: u32,
    height: u32,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let fg = binary(foreground);
    let bg = background.map(binary);
    let mut pixels: Vec<Pixel<BinaryColor>> = Vec::with_capacity((width * height) as usize);

    for py in 0..height {
        for px in 0..width {
            let idx = (py * width + px) as usize;
            let color = if *mask.get(idx).unwrap_or(&false) {
                Some(fg)
            } else {
                bg
            };

            if let Some(color) = color {
                pixels.push(Pixel(Point::new(x + px as i32, y + py as i32), color));
            }
        }
    }

    display.draw_iter(pixels.into_iter())
}

pub fn draw_page_definition<D>(
    display: &mut D,
    page: &PageDefinition,
    metrics: &SystemMetrics,
    context: &DynamicRenderContext<'_>,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    for element in &page.elements {
        match element {
            PageElement::StaticText {
                x,
                y,
                text,
                size,
                text_height_px,
                color,
                name: _,
            } => {
                let target_height = effective_text_height_px(size, *text_height_px);
                draw_scaled_text(display, text, *x, *y, target_height, color)?;
            }
            PageElement::DynamicText {
                x,
                y,
                source,
                prefix,
                max_chars,
                size,
                text_height_px,
                color,
                name: _,
            } => {
                let resolved = resolve_dynamic_text(source, page, metrics, context);
                let text = format!("{prefix}{}", truncate(&resolved, *max_chars));
                let target_height = effective_text_height_px(size, *text_height_px);
                draw_scaled_text(display, &text, *x, *y, target_height, color)?;
            }
            PageElement::Image {
                x,
                y,
                w,
                h,
                source,
                mask_mode,
                threshold,
                foreground,
                background,
                name: _,
            } => {
                if let Some(mask_image) =
                    load_monochrome_mask(source, *w, *h, *mask_mode, *threshold)
                {
                    draw_mask_image(
                        display,
                        *x,
                        *y,
                        foreground,
                        background.as_ref(),
                        &mask_image.mask,
                        mask_image.width,
                        mask_image.height,
                    )?;
                }
            }
            PageElement::Rect {
                x,
                y,
                w,
                h,
                fill,
                stroke,
                name: _,
                filled,
            } => {
                let (fill_color, stroke_color) = if fill.is_none() && stroke.is_none() {
                    if *filled {
                        (Some(BinaryColor::On), None)
                    } else {
                        (None, Some(BinaryColor::On))
                    }
                } else {
                    (fill.as_ref().map(binary), stroke.as_ref().map(binary))
                };

                let mut style_builder = PrimitiveStyleBuilder::new();
                if let Some(fill_color) = fill_color {
                    style_builder = style_builder.fill_color(fill_color);
                }
                if let Some(stroke_color) = stroke_color {
                    style_builder = style_builder.stroke_color(stroke_color).stroke_width(1);
                }
                let style = style_builder.build();

                Rectangle::new(Point::new(*x, *y), Size::new(*w, *h))
                    .into_styled(style)
                    .draw(display)?;
            }
            PageElement::Line {
                x1,
                y1,
                x2,
                y2,
                color,
                name: _,
            } => {
                Line::new(Point::new(*x1, *y1), Point::new(*x2, *y2))
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .stroke_color(binary(color))
                            .stroke_width(1)
                            .build(),
                    )
                    .draw(display)?;
            }
        }
    }

    Ok(())
}
