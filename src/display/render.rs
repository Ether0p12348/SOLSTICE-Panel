use serde::Serialize;

use crate::{
    config::AppConfig,
    display::{
        designer::{DynamicRenderContext, draw_page_definition},
        framebuffer::FrameBufferTarget,
    },
    metrics::SystemMetrics,
    page_store::PageDefinition,
    state::{AppState, DisplayMode},
};

#[derive(Debug, Clone, Serialize)]
pub struct PreviewFrame {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl PreviewFrame {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height],
        }
    }

    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, on: bool) {
        if x < 0 || y < 0 {
            return;
        }

        let x = x as usize;
        let y = y as usize;

        if x >= self.width || y >= self.height {
            return;
        }

        let idx = y * self.width + x;
        self.pixels[idx] = if on { 1 } else { 0 };
    }
}

pub fn render_page_definition_to_frame(
    width: usize,
    height: usize,
    page: &PageDefinition,
    metrics: &SystemMetrics,
    context: &DynamicRenderContext<'_>,
) -> PreviewFrame {
    let mut target = FrameBufferTarget::new(width, height);
    target.clear_buffer();

    let _ = draw_page_definition(&mut target, page, metrics, context);

    target.into_frame()
}

pub fn dynamic_render_context_from_state_config<'a>(
    state: &'a AppState,
    config: &'a AppConfig,
) -> DynamicRenderContext<'a> {
    let rotation_queue_len = state.rotation_queue.len();
    let rotation_index = if rotation_queue_len == 0 {
        None
    } else {
        Some(state.rotation_index % rotation_queue_len)
    };

    DynamicRenderContext {
        active_page_label: state.active_page.page_id(),
        display_mode_label: state.display_mode.as_str(),
        rotation_active: state.display_mode == DisplayMode::Rotating,
        rotation_interval_ms: state.rotation_interval_ms,
        rotation_queue_len,
        rotation_index,
        display_width: config.display.width,
        display_height: config.display.height,
        refresh_ms: config.display.refresh_ms,
        i2c_address: config.display.address,
        web_bind: config.web.bind.as_str(),
    }
}
