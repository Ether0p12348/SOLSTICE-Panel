use crate::display::render::PreviewFrame;
use crate::led::model::LedColor;
use crate::metrics::SystemMetrics;
use crate::page_store::PAGE_ID_LIVE_INFO;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayMode {
    #[default]
    Manual,
    Rotating,
}

impl DisplayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Rotating => "rotating",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LedRuntimeState {
    pub playing: bool,
    pub controller_mode: LedControllerModeState,
}

#[derive(Debug, Clone)]
pub struct LedControllerModeState {
    pub enabled: bool,
    pub mode: u8,
    pub speed: u8,
    pub color_index: u8,
}

impl Default for LedControllerModeState {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: 1,
            speed: 2,
            color_index: 0,
        }
    }
}

impl Default for LedRuntimeState {
    fn default() -> Self {
        Self {
            playing: false,
            controller_mode: LedControllerModeState::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PowerRuntimeState {
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

impl Default for PowerRuntimeState {
    fn default() -> Self {
        Self {
            standby_active: false,
            led_requested_on: true,
            led_effective_on: true,
            fan_requested_on: true,
            fan_effective_on: true,
            fan_auto_forced_by_temp: false,
            fan_explicit_off_temp_warning: false,
            fan_last_error: None,
            fan_auto_on_temp_c: 70,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePage {
    page_id: String,
}

impl RuntimePage {
    pub fn from_id(id: impl Into<String>) -> Self {
        Self { page_id: id.into() }
    }

    pub fn page_id(&self) -> &str {
        self.page_id.as_str()
    }

    pub fn label(&self) -> String {
        self.page_id.clone()
    }
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub metrics: SystemMetrics,
    pub active_page: RuntimePage,
    pub display_mode: DisplayMode,
    pub rotation_interval_ms: u64,
    pub rotation_queue: Vec<RuntimePage>,
    pub rotation_index: usize,
    pub led_runtime: LedRuntimeState,
    pub led_output: LedOutputState,
    pub power: PowerRuntimeState,
}

#[derive(Debug, Clone)]
pub struct LedOutputState {
    pub hardware_led_count: u16,
    pub frame: Vec<LedColor>,
    pub source_project_id: Option<String>,
    pub execution_mode: LedExecutionMode,
    pub frame_is_estimated: bool,
    pub offloaded: Option<LedOffloadedState>,
    pub physically_synced: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedExecutionMode {
    Manual,
    Offloaded,
}

impl LedExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Offloaded => "offloaded",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LedOffloadedState {
    pub mode: u8,
    pub speed: u8,
    pub color_index: u8,
    pub source_effect_id: String,
}

impl Default for LedOutputState {
    fn default() -> Self {
        Self {
            hardware_led_count: 0,
            frame: Vec::new(),
            source_project_id: None,
            execution_mode: LedExecutionMode::Manual,
            frame_is_estimated: false,
            offloaded: None,
            physically_synced: false,
            last_error: None,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            metrics: SystemMetrics::default(),
            active_page: RuntimePage::from_id(PAGE_ID_LIVE_INFO),
            display_mode: DisplayMode::Manual,
            rotation_interval_ms: 5000,
            rotation_queue: vec![RuntimePage::from_id(PAGE_ID_LIVE_INFO)],
            rotation_index: 0,
            led_runtime: LedRuntimeState::default(),
            led_output: LedOutputState::default(),
            power: PowerRuntimeState::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UiSnapshot {
    pub hostname: String,
    pub ip_addr: String,
    pub uptime_text: String,
    pub ram_percent_text: String,
    pub cpu_temp_text: String,
    pub active_page: String,
    pub display_mode: String,
    pub rotation_interval_ms: u64,
    pub rotation_queue: Vec<String>,
    pub rotation_active: bool,
    pub rotation_queue_len: usize,
    pub rotation_index: Option<usize>,
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

impl UiSnapshot {
    pub fn from_state(state: &AppState) -> Self {
        let rotation_queue_len = state.rotation_queue.len();
        let rotation_index = if rotation_queue_len == 0 {
            None
        } else {
            Some(state.rotation_index % rotation_queue_len)
        };

        Self {
            hostname: state.metrics.hostname.clone(),
            ip_addr: state.metrics.ip_addr.clone(),
            uptime_text: state.metrics.uptime_text.clone(),
            ram_percent_text: state.metrics.ram_percent_text.clone(),
            cpu_temp_text: state.metrics.cpu_temp_text.clone(),
            active_page: state.active_page.label(),
            display_mode: state.display_mode.as_str().to_string(),
            rotation_interval_ms: state.rotation_interval_ms,
            rotation_queue: state.rotation_queue.iter().map(|p| p.label()).collect(),
            rotation_active: state.display_mode == DisplayMode::Rotating,
            rotation_queue_len,
            rotation_index,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct PreviewSnapshot {
    pub active_page: String,
    pub frame: PreviewFrame,
}

#[derive(Debug, Clone)]
pub enum ServerEvent {
    Status(UiSnapshot),
    Preview(PreviewSnapshot),
    Invalidate,
}
