use crate::{
    config::AppConfig,
    config_store::{ConfigStore, SharedConfigStore},
    display::{
        oled::OledPanel,
        render::{dynamic_render_context_from_state_config, render_page_definition_to_frame},
    },
    led::{
        lab::{LedLabRunner, LedLabStore, SharedLedLabRunner, SharedLedLabStore},
        model::LedColor,
        offload::{BuiltinLedProgram, estimated_frame},
        strip::YahboomLedStrip,
    },
    metrics::collect_metrics,
    page_store::{PAGE_ID_LIVE_INFO, PageStore, SharedPageStore},
    published_store::{PublishedDisplaySpec, PublishedStore, SharedPublishedStore},
    state::{
        AppState, DisplayMode, LedExecutionMode, LedOffloadedState, PreviewSnapshot, RuntimePage,
        ServerEvent, UiSnapshot,
    },
    web::routes::{WebContext, router},
};
use anyhow::{Context, Result};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    net::TcpListener,
    sync::{RwLock, broadcast},
    time::sleep,
};
use tracing::{error, info, warn};

const LED_FAILURE_AUTO_PAUSE_THRESHOLD: u32 = 4;
const LED_BACKOFF_BASE_SECS: u64 = 2;
const LED_BACKOFF_MAX_SECS: u64 = 30;
const LED_RUNTIME_TICK_MS: u64 = 125;
const OLED_BACKOFF_BASE_MS: u64 = 250;
const OLED_BACKOFF_MAX_MS: u64 = 5_000;
const SHARED_BUS_RECOVERY_AFTER_LED_FAILURE_MS: u64 = 750;
const SHARED_BUS_SECONDARY_WINDOW_MS: u64 = 3_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SharedBusFaultOrigin {
    Led,
    Oled,
}

fn exponential_backoff(base: Duration, max: Duration, failures: u32) -> Duration {
    let shift = failures.saturating_sub(1).min(8);
    let scaled = base.saturating_mul(1_u32 << shift);
    if scaled > max { max } else { scaled }
}

fn parse_cpu_temp_celsius(raw: &str) -> Option<f32> {
    let trimmed = raw.trim();
    let value_text = trimmed.strip_suffix('C').unwrap_or(trimmed).trim();
    value_text.parse::<f32>().ok()
}

fn led_palette_color(color_index: u8) -> LedColor {
    match color_index {
        0 => LedColor::rgb(255, 0, 0),
        1 => LedColor::rgb(0, 255, 0),
        2 => LedColor::rgb(0, 0, 255),
        3 => LedColor::rgb(255, 255, 0),
        4 => LedColor::rgb(255, 0, 255),
        5 => LedColor::rgb(0, 255, 255),
        _ => LedColor::rgb(255, 255, 255),
    }
}

pub struct App {
    pub config_store: SharedConfigStore,
    pub page_store: SharedPageStore,
    pub published_store: SharedPublishedStore,
    pub led_lab_store: SharedLedLabStore,
    pub led_lab_runner: SharedLedLabRunner,
    pub state: Arc<RwLock<AppState>>,
    pub event_tx: broadcast::Sender<ServerEvent>,
}

impl App {
    pub fn new(
        config_path: &str,
        pages_path: &str,
        published_path: &str,
        led_lab_path: &str,
        config: AppConfig,
    ) -> Result<Self> {
        let page_store_raw =
            PageStore::load_or_create(pages_path, config.display.width, config.display.height)?;
        let published_store_raw = PublishedStore::load_or_create(published_path)?;
        let led_lab_store_raw = LedLabStore::load_or_create(led_lab_path)?;
        let initial_hw_led_count = config.led.hardware_led_count.max(1);
        let initial_fan_auto_on_temp_c = config.led.fan_auto_on_temp_c;
        let initial_default_effect_mode = config.led.default_effect_mode();

        let config_store = Arc::new(RwLock::new(ConfigStore {
            path: config_path.into(),
            config,
        }));

        let mut initial_state = AppState::default();
        published_store_raw
            .spec
            .apply_to_runtime_state(&mut initial_state);
        let default_page_id = page_store_raw
            .pages
            .first()
            .map(|page| page.id.clone())
            .unwrap_or_else(|| PAGE_ID_LIVE_INFO.to_string());
        if initial_state.rotation_queue.is_empty() {
            initial_state.rotation_queue = vec![RuntimePage::from_id(default_page_id.clone())];
            initial_state.active_page = RuntimePage::from_id(default_page_id.clone());
        } else if !page_store_raw
            .pages
            .iter()
            .any(|page| page.id == initial_state.active_page.page_id())
        {
            initial_state.active_page = RuntimePage::from_id(default_page_id);
        }
        initial_state.led_output.hardware_led_count = initial_hw_led_count;
        initial_state.led_output.frame =
            vec![LedColor::rgb(0, 0, 0); initial_hw_led_count as usize];
        initial_state.led_output.source_project_id = None;
        initial_state.led_output.execution_mode = LedExecutionMode::Manual;
        initial_state.led_output.frame_is_estimated = false;
        initial_state.led_output.offloaded = None;
        initial_state.led_output.physically_synced = true;
        initial_state.led_output.last_error = None;
        initial_state.led_runtime.controller_mode.enabled = true;
        initial_state.led_runtime.controller_mode.mode = initial_default_effect_mode;
        initial_state.led_runtime.playing = true;
        initial_state.power.fan_auto_on_temp_c = initial_fan_auto_on_temp_c;
        initial_state.power.led_requested_on = true;
        initial_state.power.led_effective_on = true;

        let page_store = Arc::new(RwLock::new(page_store_raw));
        let published_store = Arc::new(RwLock::new(published_store_raw));
        let led_lab_store = Arc::new(RwLock::new(led_lab_store_raw));
        let led_lab_runner = Arc::new(LedLabRunner::new());
        let state = Arc::new(RwLock::new(initial_state));

        let (event_tx, _) = broadcast::channel(128);

        Ok(Self {
            config_store,
            page_store,
            published_store,
            led_lab_store,
            led_lab_runner,
            state,
            event_tx,
        })
    }

    pub async fn run(&self) -> Result<()> {
        let web_ctx = WebContext {
            state: self.state.clone(),
            config_store: self.config_store.clone(),
            page_store: self.page_store.clone(),
            published_store: self.published_store.clone(),
            led_lab_store: self.led_lab_store.clone(),
            led_lab_runner: self.led_lab_runner.clone(),
            event_tx: self.event_tx.clone(),
        };

        let web_bind = {
            let store = self.config_store.read().await;
            store.config.web.bind.clone()
        };

        self.publish_ui_snapshots().await;

        let web_task = tokio::spawn(async move { run_web_server(web_bind, web_ctx).await });

        let display_result = self.run_display_loop().await;

        if let Err(err) = &display_result {
            error!(error = %err, "display loop exited with error");
        }

        web_task.abort();

        match web_task.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => error!(error = %err, "web server exited with error"),
            Err(_) => info!("web server task stopped"),
        }

        display_result
    }

    async fn run_display_loop(&self) -> Result<()> {
        let mut config = {
            let store = self.config_store.read().await;
            store.config.clone()
        };

        let spec = { self.published_store.read().await.spec.clone() };

        let mut display_enabled = config.display.enabled;
        let mut led_runtime_enabled = config.led.enabled;
        let mut shared_i2c_bus = display_enabled
            && led_runtime_enabled
            && config.display.i2c_path == config.led.i2c_path;

        if shared_i2c_bus {
            warn!(
                i2c_path = %config.display.i2c_path,
                oled_address = format!("0x{:02X}", config.display.address),
                led_address = format!("0x{:02X}", config.led.address),
                "OLED and LED runtime share the same I2C bus; enabling coordinated fault recovery"
            );
        }

        let mut oled = None;
        let mut oled_consecutive_failures: u32 = 0;
        let mut oled_retry_not_before = tokio::time::Instant::now();

        if display_enabled {
            match OledPanel::new(&config.display) {
                Ok(mut panel) => {
                    if let Err(err) = self.run_boot_sequence(&mut panel, &spec).await {
                        oled_consecutive_failures = 1;
                        let backoff = exponential_backoff(
                            Duration::from_millis(OLED_BACKOFF_BASE_MS),
                            Duration::from_millis(OLED_BACKOFF_MAX_MS),
                            oled_consecutive_failures,
                        );
                        oled_retry_not_before = tokio::time::Instant::now() + backoff;
                        error!(
                            error = %err,
                            i2c_path = %config.display.i2c_path,
                            address = format!("0x{:02X}", config.display.address),
                            reinit_in_ms = backoff.as_millis() as u64,
                            "OLED boot sequence failed; OLED output will retry with backoff"
                        );
                    } else {
                        oled = Some(panel);
                    }
                }
                Err(err) => {
                    oled_consecutive_failures = 1;
                    let backoff = exponential_backoff(
                        Duration::from_millis(OLED_BACKOFF_BASE_MS),
                        Duration::from_millis(OLED_BACKOFF_MAX_MS),
                        oled_consecutive_failures,
                    );
                    oled_retry_not_before = tokio::time::Instant::now() + backoff;
                    error!(
                        error = %err,
                        i2c_path = %config.display.i2c_path,
                        address = format!("0x{:02X}", config.display.address),
                        reinit_in_ms = backoff.as_millis() as u64,
                        "failed to initialize OLED panel; OLED output will retry with backoff"
                    );
                }
            }
        } else {
            info!("display disabled in config; OLED output loop is inactive");
        }

        {
            let mut state = self.state.write().await;
            spec.apply_to_runtime_state(&mut state);
        }

        self.publish_ui_snapshots().await;

        let mut render_tick =
            tokio::time::interval(Duration::from_millis(config.display.refresh_ms));
        let mut led_tick = tokio::time::interval(Duration::from_millis(LED_RUNTIME_TICK_MS));
        let mut led_strip: Option<YahboomLedStrip> = None;
        let mut led_is_cleared = false;
        let mut led_retry_not_before = tokio::time::Instant::now();
        let mut led_consecutive_failures: u32 = 0;
        let mut last_shared_bus_fault: Option<(SharedBusFaultOrigin, tokio::time::Instant)> = None;
        let mut hardware_led_count = config.led.hardware_led_count.max(1);
        let mut led_observed_frame = vec![LedColor::rgb(0, 0, 0); hardware_led_count as usize];
        let mut led_output_source_project_id: Option<String> = None;
        let mut led_output_physically_synced = true;
        let mut led_output_last_error: Option<String> = None;
        let mut last_applied_fan_state: Option<bool> = None;
        let mut last_led_execution_mode: Option<LedExecutionMode> = None;
        let mut last_controller_mode_signature: Option<(u8, u8, u8)> = None;

        self.set_led_output_state(
            hardware_led_count,
            led_observed_frame.clone(),
            led_output_source_project_id.clone(),
            led_output_physically_synced,
            led_output_last_error.clone(),
        )
        .await;

        let initial_rotation_interval = {
            let state = self.state.read().await;
            state.rotation_interval_ms
        };

        let mut current_rotation_interval = initial_rotation_interval;
        let mut next_rotation_at =
            tokio::time::Instant::now() + Duration::from_millis(current_rotation_interval);

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("shutdown signal received");
                    if let Some(oled_panel) = oled.as_mut() {
                        if let Err(err) = oled_panel.clear() {
                            error!(error = %err, "failed to clear OLED on shutdown");
                        }
                    }
                    if let Some(strip) = led_strip.as_mut() {
                        if let Err(err) = strip.clear() {
                            error!(error = %err, "failed to clear LED strip on shutdown");
                        }
                    }
                    return Ok(());
                }

                _ = render_tick.tick() => {
                    match collect_metrics() {
                        Ok(metrics) => {
                            {
                                let mut state = self.state.write().await;
                                state.metrics = metrics;
                            }

                            let latest_config = {
                                let store = self.config_store.read().await;
                                store.config.clone()
                            };

                            if latest_config != config {
                                let previous = config.clone();
                                config = latest_config;
                                display_enabled = config.display.enabled;
                                led_runtime_enabled = config.led.enabled;
                                shared_i2c_bus = display_enabled
                                    && led_runtime_enabled
                                    && config.display.i2c_path == config.led.i2c_path;

                                if previous.display.refresh_ms != config.display.refresh_ms {
                                    render_tick = tokio::time::interval(Duration::from_millis(
                                        config.display.refresh_ms,
                                    ));
                                }
                                if previous.display.enabled != config.display.enabled
                                    || previous.display.i2c_path != config.display.i2c_path
                                    || previous.display.address != config.display.address
                                {
                                    oled = None;
                                    oled_consecutive_failures = 0;
                                    oled_retry_not_before = tokio::time::Instant::now();
                                }

                                if previous.led.enabled != config.led.enabled
                                    || previous.led.i2c_path != config.led.i2c_path
                                    || previous.led.address != config.led.address
                                    || previous.led.hardware_led_count
                                        != config.led.hardware_led_count
                                {
                                    led_strip = None;
                                    led_retry_not_before = tokio::time::Instant::now();
                                    led_consecutive_failures = 0;
                                    last_applied_fan_state = None;
                                    led_is_cleared = false;
                                    last_led_execution_mode = None;
                                    last_controller_mode_signature = None;
                                }

                                if previous.led.hardware_led_count != config.led.hardware_led_count {
                                    hardware_led_count = config.led.hardware_led_count.max(1);
                                    let target_len = hardware_led_count as usize;
                                    if led_observed_frame.len() > target_len {
                                        led_observed_frame.truncate(target_len);
                                    } else if led_observed_frame.len() < target_len {
                                        led_observed_frame
                                            .resize(target_len, LedColor::rgb(0, 0, 0));
                                    }
                                    self.set_led_output_state(
                                        hardware_led_count,
                                        led_observed_frame.clone(),
                                        led_output_source_project_id.clone(),
                                        led_output_physically_synced,
                                        led_output_last_error.clone(),
                                    )
                                    .await;
                                }

                                if previous.led.fan_auto_on_temp_c != config.led.fan_auto_on_temp_c
                                {
                                    let mut state = self.state.write().await;
                                    state.power.fan_auto_on_temp_c = config.led.fan_auto_on_temp_c;
                                }
                                if previous.led.default_effect_id != config.led.default_effect_id {
                                    let mut state = self.state.write().await;
                                    state.led_runtime.controller_mode.mode =
                                        config.led.default_effect_mode();
                                    state.led_runtime.controller_mode.enabled = true;
                                    state.led_runtime.playing = true;
                                }

                                info!(
                                    display_enabled = config.display.enabled,
                                    display_i2c_path = %config.display.i2c_path,
                                    display_address = format!("0x{:02X}", config.display.address),
                                    refresh_ms = config.display.refresh_ms,
                                    led_enabled = config.led.enabled,
                                    led_i2c_path = %config.led.i2c_path,
                                    led_address = format!("0x{:02X}", config.led.address),
                                    led_count = config.led.hardware_led_count,
                                    fan_auto_on_temp_c = config.led.fan_auto_on_temp_c,
                                    "applied updated runtime configuration"
                                );
                            }

                            let now = tokio::time::Instant::now();
                            if display_enabled && oled.is_none() && now >= oled_retry_not_before {
                                match OledPanel::new(&config.display) {
                                    Ok(panel) => {
                                        info!(
                                            i2c_path = %config.display.i2c_path,
                                            address = format!("0x{:02X}", config.display.address),
                                            previous_failures = oled_consecutive_failures,
                                            "OLED transport reinitialized"
                                        );
                                        oled = Some(panel);
                                        oled_consecutive_failures = 0;
                                    }
                                    Err(err) => {
                                        oled_consecutive_failures =
                                            oled_consecutive_failures.saturating_add(1);
                                        let backoff = exponential_backoff(
                                            Duration::from_millis(OLED_BACKOFF_BASE_MS),
                                            Duration::from_millis(OLED_BACKOFF_MAX_MS),
                                            oled_consecutive_failures,
                                        );
                                        oled_retry_not_before = tokio::time::Instant::now() + backoff;
                                        error!(
                                            error = %err,
                                            i2c_path = %config.display.i2c_path,
                                            address = format!("0x{:02X}", config.display.address),
                                            consecutive_failures = oled_consecutive_failures,
                                            reinit_in_ms = backoff.as_millis() as u64,
                                            "failed to reinitialize OLED transport"
                                        );
                                    }
                                }
                            }

                            if let Some(oled_panel) = oled.as_mut() {
                                let frame = self.build_live_frame().await;
                                if let Err(err) = oled_panel.show_frame(&frame) {
                                    oled_consecutive_failures =
                                        oled_consecutive_failures.saturating_add(1);
                                    let backoff = exponential_backoff(
                                        Duration::from_millis(OLED_BACKOFF_BASE_MS),
                                        Duration::from_millis(OLED_BACKOFF_MAX_MS),
                                        oled_consecutive_failures,
                                    );
                                    let now = tokio::time::Instant::now();
                                    oled_retry_not_before = now + backoff;
                                    let likely_secondary_to_led_fault = shared_i2c_bus
                                        && last_shared_bus_fault.is_some_and(|(origin, fault_at)| {
                                            origin == SharedBusFaultOrigin::Led
                                                && now.duration_since(fault_at)
                                                    <= Duration::from_millis(
                                                        SHARED_BUS_SECONDARY_WINDOW_MS,
                                                    )
                                        });
                                    error!(
                                        error = %err,
                                        i2c_path = %config.display.i2c_path,
                                        address = format!("0x{:02X}", config.display.address),
                                        consecutive_failures = oled_consecutive_failures,
                                        reinit_in_ms = backoff.as_millis() as u64,
                                        likely_secondary_to_led_fault,
                                        "failed to render OLED frame; dropping OLED handle and retrying"
                                    );
                                    last_shared_bus_fault = Some((SharedBusFaultOrigin::Oled, now));
                                    oled = None;
                                } else {
                                    oled_consecutive_failures = 0;
                                }
                            }

                            self.publish_ui_snapshots().await;
                        }
                        Err(err) => {
                            error!(error = %err, "failed to collect metrics");
                        }
                    }
                }

                _ = tokio::time::sleep_until(next_rotation_at) => {
                    let mut changed = false;

                    let latest_spec = self.published_store.read().await.spec.clone();

                    {
                        let mut state = self.state.write().await;

                        let latest_queue = latest_spec.runtime_queue();
                        if latest_queue != state.rotation_queue {
                            state.rotation_queue = latest_queue;
                            state.rotation_index = 0;
                            changed = true;
                        }

                        current_rotation_interval = state.rotation_interval_ms;

                        if state.display_mode == DisplayMode::Rotating && !state.rotation_queue.is_empty() {
                            state.rotation_index = (state.rotation_index + 1) % state.rotation_queue.len();
                            state.active_page = state.rotation_queue[state.rotation_index].clone();
                            changed = true;
                        }
                    }

                    next_rotation_at =
                        tokio::time::Instant::now() + Duration::from_millis(current_rotation_interval);

                    if changed {
                        self.publish_ui_snapshots().await;
                    }
                }

                _ = led_tick.tick() => {
                    let now = tokio::time::Instant::now();
                    if now < led_retry_not_before {
                        continue;
                    }
                    if shared_i2c_bus && now < oled_retry_not_before {
                        continue;
                    }

                    let (led_runtime, power_runtime) = {
                        let mut state = self.state.write().await;
                        let cpu_temp_c = parse_cpu_temp_celsius(&state.metrics.cpu_temp_text);
                        let temp_threshold = f32::from(state.power.fan_auto_on_temp_c);
                        let temp_over_threshold =
                            cpu_temp_c.is_some_and(|temp| temp >= temp_threshold);

                        let fan_auto_forced_by_temp = state.power.standby_active && temp_over_threshold;
                        let fan_explicit_off_temp_warning = !state.power.standby_active
                            && !state.power.fan_requested_on
                            && temp_over_threshold;

                        state.power.fan_auto_forced_by_temp = fan_auto_forced_by_temp;
                        state.power.fan_explicit_off_temp_warning = fan_explicit_off_temp_warning;
                        state.power.led_effective_on =
                            state.power.led_requested_on && !state.power.standby_active;
                        state.power.fan_effective_on = if state.power.standby_active {
                            fan_auto_forced_by_temp
                        } else {
                            state.power.fan_requested_on
                        };

                        (state.led_runtime.clone(), state.power.clone())
                    };

                    let led_should_render =
                        led_runtime_enabled && led_runtime.playing && power_runtime.led_effective_on;

                    if led_strip.is_none()
                        && (last_applied_fan_state != Some(power_runtime.fan_effective_on)
                            || led_should_render)
                    {
                        match YahboomLedStrip::new(
                            &config.led.i2c_path,
                            config.led.address,
                            config.led.hardware_led_count,
                        ) {
                            Ok(strip) => {
                                led_strip = Some(strip);
                            }
                            Err(err) => {
                                let err_text = err.to_string();
                                {
                                    let mut state = self.state.write().await;
                                    state.power.fan_last_error = Some(err_text.clone());
                                }
                                led_output_physically_synced = false;
                                led_output_last_error = Some(err_text);
                                self.set_led_output_state(
                                    hardware_led_count,
                                    led_observed_frame.clone(),
                                    led_output_source_project_id.clone(),
                                    led_output_physically_synced,
                                    led_output_last_error.clone(),
                                )
                                .await;
                                let now = tokio::time::Instant::now();
                                led_consecutive_failures = led_consecutive_failures.saturating_add(1);
                                let backoff = exponential_backoff(
                                    Duration::from_secs(LED_BACKOFF_BASE_SECS),
                                    Duration::from_secs(LED_BACKOFF_MAX_SECS),
                                    led_consecutive_failures,
                                );
                                led_retry_not_before = now + backoff;
                                error!(
                                    error = %err,
                                    i2c_path = %config.led.i2c_path,
                                    address = format!("0x{:02X}", config.led.address),
                                    consecutive_failures = led_consecutive_failures,
                                    retry_in_ms = backoff.as_millis() as u64,
                                    "failed to initialize LED strip driver for fan/runtime control"
                                );
                                last_shared_bus_fault = Some((SharedBusFaultOrigin::Led, now));
                                if shared_i2c_bus {
                                    let recovery_delay = Duration::from_millis(
                                        SHARED_BUS_RECOVERY_AFTER_LED_FAILURE_MS,
                                    );
                                    let target = now + recovery_delay;
                                    if target > oled_retry_not_before {
                                        oled_retry_not_before = target;
                                    }
                                    if oled.is_some() {
                                        warn!(
                                            i2c_path = %config.display.i2c_path,
                                            led_address = format!("0x{:02X}", config.led.address),
                                            oled_address = format!("0x{:02X}", config.display.address),
                                            oled_reinit_in_ms = recovery_delay.as_millis() as u64,
                                            "LED failure detected on shared I2C bus; forcing OLED reinit window"
                                        );
                                    }
                                    oled = None;
                                }
                                last_led_execution_mode = None;
                                last_controller_mode_signature = None;
                                last_applied_fan_state = None;
                                continue;
                            }
                        }
                    }

                    if let Some(strip) = led_strip.as_mut() {
                        if last_applied_fan_state != Some(power_runtime.fan_effective_on) {
                            match strip.set_fan_enabled(power_runtime.fan_effective_on) {
                                Ok(_) => {
                                    last_applied_fan_state = Some(power_runtime.fan_effective_on);
                                    let mut state = self.state.write().await;
                                    state.power.fan_last_error = None;
                                }
                                Err(err) => {
                                    let err_text = err.to_string();
                                    {
                                        let mut state = self.state.write().await;
                                        state.power.fan_last_error = Some(err_text.clone());
                                    }
                                    let now = tokio::time::Instant::now();
                                    led_consecutive_failures = led_consecutive_failures.saturating_add(1);
                                    let backoff = exponential_backoff(
                                        Duration::from_secs(LED_BACKOFF_BASE_SECS),
                                        Duration::from_secs(LED_BACKOFF_MAX_SECS),
                                        led_consecutive_failures,
                                    );
                                    led_retry_not_before = now + backoff;
                                    error!(
                                        error = %err,
                                        i2c_path = %config.led.i2c_path,
                                        address = format!("0x{:02X}", config.led.address),
                                        desired_on = power_runtime.fan_effective_on,
                                        consecutive_failures = led_consecutive_failures,
                                        retry_in_ms = backoff.as_millis() as u64,
                                        "failed to apply fan state"
                                    );
                                    last_shared_bus_fault = Some((SharedBusFaultOrigin::Led, now));
                                    if shared_i2c_bus {
                                        let recovery_delay = Duration::from_millis(
                                            SHARED_BUS_RECOVERY_AFTER_LED_FAILURE_MS,
                                        );
                                        let target = now + recovery_delay;
                                        if target > oled_retry_not_before {
                                            oled_retry_not_before = target;
                                        }
                                        oled = None;
                                    }
                                    led_strip = None;
                                    last_applied_fan_state = None;
                                    led_is_cleared = true;
                                    last_led_execution_mode = None;
                                    last_controller_mode_signature = None;
                                    continue;
                                }
                            }
                        }
                    }

                    if !led_should_render {
                        if !led_is_cleared {
                            if let Some(strip) = led_strip.as_mut() {
                                if let Err(err) = strip.clear() {
                                    led_observed_frame = strip.snapshot_frame();
                                    led_output_source_project_id = None;
                                    led_output_physically_synced = false;
                                    led_output_last_error = Some(err.to_string());
                                    self.set_led_output_state(
                                        hardware_led_count,
                                        led_observed_frame.clone(),
                                        led_output_source_project_id.clone(),
                                        led_output_physically_synced,
                                        led_output_last_error.clone(),
                                    )
                                    .await;
                                    let now = tokio::time::Instant::now();
                                    led_consecutive_failures = led_consecutive_failures.saturating_add(1);
                                    let backoff = exponential_backoff(
                                        Duration::from_secs(LED_BACKOFF_BASE_SECS),
                                        Duration::from_secs(LED_BACKOFF_MAX_SECS),
                                        led_consecutive_failures,
                                    );
                                    led_retry_not_before = now + backoff;
                                    error!(
                                        error = %err,
                                        i2c_path = %config.led.i2c_path,
                                        address = format!("0x{:02X}", config.led.address),
                                        consecutive_failures = led_consecutive_failures,
                                        retry_in_ms = backoff.as_millis() as u64,
                                        "failed clearing LED strip while paused/standby"
                                    );
                                    last_shared_bus_fault = Some((SharedBusFaultOrigin::Led, now));
                                    if shared_i2c_bus {
                                        let recovery_delay = Duration::from_millis(
                                            SHARED_BUS_RECOVERY_AFTER_LED_FAILURE_MS,
                                        );
                                        let target = now + recovery_delay;
                                        if target > oled_retry_not_before {
                                            oled_retry_not_before = target;
                                        }
                                        oled = None;
                                    }
                                    led_strip = None;
                                    last_applied_fan_state = None;
                                } else {
                                    led_observed_frame = strip.snapshot_frame();
                                    led_output_source_project_id = None;
                                    led_output_physically_synced = true;
                                    led_output_last_error = None;
                                    self.set_led_output_state(
                                        hardware_led_count,
                                        led_observed_frame.clone(),
                                        led_output_source_project_id.clone(),
                                        led_output_physically_synced,
                                        led_output_last_error.clone(),
                                    )
                                    .await;
                                }
                            }
                            led_is_cleared = true;
                        }
                        last_led_execution_mode = None;
                        last_controller_mode_signature = None;
                        continue;
                    }

                    if led_runtime.controller_mode.enabled {
                        if led_runtime.controller_mode.mode == 0 {
                            let signature = (
                                led_runtime.controller_mode.mode,
                                led_runtime.controller_mode.speed,
                                led_runtime.controller_mode.color_index,
                            );

                            if let Some(strip) = led_strip.as_mut() {
                                led_observed_frame = strip.snapshot_frame();
                                self.set_led_output_state(
                                    hardware_led_count,
                                    led_observed_frame.clone(),
                                    None,
                                    true,
                                    None,
                                )
                                .await;
                                led_is_cleared = false;
                                led_consecutive_failures = 0;
                                last_led_execution_mode = Some(LedExecutionMode::Manual);
                                last_controller_mode_signature = Some(signature);
                                continue;
                            }
                        }

                        let override_program = BuiltinLedProgram {
                            mode: led_runtime.controller_mode.mode,
                            speed: led_runtime.controller_mode.speed,
                            color_index: led_runtime.controller_mode.color_index,
                            estimated_color: led_palette_color(led_runtime.controller_mode.color_index),
                            source_effect_id: "controller_mode_override".to_string(),
                        };

                        if let Some(strip) = led_strip.as_mut() {
                            match strip.set_builtin_effect(&override_program) {
                                Ok(changed) => {
                                    let signature = (
                                        override_program.mode,
                                        override_program.speed,
                                        override_program.color_index,
                                    );
                                    if changed
                                        || last_led_execution_mode != Some(LedExecutionMode::Offloaded)
                                        || last_controller_mode_signature.as_ref() != Some(&signature)
                                    {
                                        info!(
                                            mode = override_program.mode,
                                            speed = override_program.speed,
                                            color_index = override_program.color_index,
                                            programmed = changed,
                                            "using LED controller-mode override path"
                                        );
                                    }

                                    let estimated =
                                        estimated_frame(&override_program, hardware_led_count as usize);
                                    self.set_led_output_offloaded_state(
                                        hardware_led_count,
                                        estimated,
                                        None,
                                        &override_program,
                                        None,
                                    )
                                    .await;
                                    led_is_cleared = false;
                                    led_consecutive_failures = 0;
                                    last_led_execution_mode = Some(LedExecutionMode::Offloaded);
                                    last_controller_mode_signature = Some(signature);
                                    continue;
                                }
                                Err(err) => {
                                    let err_text = err.to_string();
                                    self.set_led_output_offloaded_state(
                                        hardware_led_count,
                                        estimated_frame(&override_program, hardware_led_count as usize),
                                        None,
                                        &override_program,
                                        Some(err_text.clone()),
                                    )
                                    .await;

                                    let now = tokio::time::Instant::now();
                                    led_consecutive_failures = led_consecutive_failures.saturating_add(1);
                                    let backoff = exponential_backoff(
                                        Duration::from_secs(LED_BACKOFF_BASE_SECS),
                                        Duration::from_secs(LED_BACKOFF_MAX_SECS),
                                        led_consecutive_failures,
                                    );
                                    led_retry_not_before = now + backoff;
                                    error!(
                                        error = %err,
                                        i2c_path = %config.led.i2c_path,
                                        address = format!("0x{:02X}", config.led.address),
                                        consecutive_failures = led_consecutive_failures,
                                        retry_in_ms = backoff.as_millis() as u64,
                                        "failed to apply LED controller-mode override"
                                    );
                                    last_shared_bus_fault = Some((SharedBusFaultOrigin::Led, now));

                                    if shared_i2c_bus {
                                        let recovery_delay =
                                            Duration::from_millis(SHARED_BUS_RECOVERY_AFTER_LED_FAILURE_MS);
                                        let target = now + recovery_delay;
                                        if target > oled_retry_not_before {
                                            oled_retry_not_before = target;
                                        }
                                        if oled.is_some() {
                                            warn!(
                                                i2c_path = %config.display.i2c_path,
                                                led_address = format!("0x{:02X}", config.led.address),
                                                oled_address = format!("0x{:02X}", config.display.address),
                                                oled_reinit_in_ms = recovery_delay.as_millis() as u64,
                                                "LED override write failed on shared I2C bus; dropping OLED handle for recovery"
                                            );
                                        }
                                        oled = None;
                                    }

                                    led_strip = None;
                                    last_applied_fan_state = None;
                                    led_is_cleared = true;
                                    last_led_execution_mode = None;
                                    last_controller_mode_signature = None;

                                    if led_consecutive_failures >= LED_FAILURE_AUTO_PAUSE_THRESHOLD {
                                        {
                                            let mut state = self.state.write().await;
                                            state.led_runtime.playing = false;
                                        }
                                        warn!(
                                            i2c_path = %config.led.i2c_path,
                                            address = format!("0x{:02X}", config.led.address),
                                            failures = led_consecutive_failures,
                                            "auto-pausing LED runtime after repeated hardware failures"
                                        );
                                        led_consecutive_failures = 0;
                                    }
                                    continue;
                                }
                            }
                        }
                    }

                    if !led_is_cleared {
                        if let Some(strip) = led_strip.as_mut() {
                            if let Err(err) = strip.clear() {
                                led_observed_frame = strip.snapshot_frame();
                                led_output_source_project_id = None;
                                led_output_physically_synced = false;
                                led_output_last_error = Some(err.to_string());
                                self.set_led_output_state(
                                    hardware_led_count,
                                    led_observed_frame.clone(),
                                    led_output_source_project_id.clone(),
                                    led_output_physically_synced,
                                    led_output_last_error.clone(),
                                )
                                .await;
                                let now = tokio::time::Instant::now();
                                led_consecutive_failures = led_consecutive_failures.saturating_add(1);
                                let backoff = exponential_backoff(
                                    Duration::from_secs(LED_BACKOFF_BASE_SECS),
                                    Duration::from_secs(LED_BACKOFF_MAX_SECS),
                                    led_consecutive_failures,
                                );
                                led_retry_not_before = now + backoff;
                                error!(
                                    error = %err,
                                    i2c_path = %config.led.i2c_path,
                                    address = format!("0x{:02X}", config.led.address),
                                    consecutive_failures = led_consecutive_failures,
                                    retry_in_ms = backoff.as_millis() as u64,
                                    "failed clearing LED strip while controller mode is disabled"
                                );
                                last_shared_bus_fault = Some((SharedBusFaultOrigin::Led, now));
                                if shared_i2c_bus {
                                    let recovery_delay = Duration::from_millis(
                                        SHARED_BUS_RECOVERY_AFTER_LED_FAILURE_MS,
                                    );
                                    let target = now + recovery_delay;
                                    if target > oled_retry_not_before {
                                        oled_retry_not_before = target;
                                    }
                                    oled = None;
                                }
                                led_strip = None;
                                last_applied_fan_state = None;
                            } else {
                                led_observed_frame = strip.snapshot_frame();
                                led_output_source_project_id = None;
                                led_output_physically_synced = true;
                                led_output_last_error = None;
                                self.set_led_output_state(
                                    hardware_led_count,
                                    led_observed_frame.clone(),
                                    led_output_source_project_id.clone(),
                                    led_output_physically_synced,
                                    led_output_last_error.clone(),
                                )
                                .await;
                            }
                        }
                        led_is_cleared = true;
                    }
                    last_led_execution_mode = None;
                    last_controller_mode_signature = None;
                    continue;
                }
            }
        }
    }

    async fn set_led_output_state(
        &self,
        hardware_led_count: u16,
        mut frame: Vec<LedColor>,
        source_project_id: Option<String>,
        physically_synced: bool,
        last_error: Option<String>,
    ) {
        let normalized_count = hardware_led_count.max(1);
        let target_len = normalized_count as usize;
        if frame.len() > target_len {
            frame.truncate(target_len);
        } else if frame.len() < target_len {
            frame.resize(target_len, LedColor::rgb(0, 0, 0));
        }

        let mut state = self.state.write().await;
        state.led_output.hardware_led_count = normalized_count;
        state.led_output.frame = frame;
        state.led_output.source_project_id = source_project_id;
        state.led_output.execution_mode = LedExecutionMode::Manual;
        state.led_output.frame_is_estimated = false;
        state.led_output.offloaded = None;
        state.led_output.physically_synced = physically_synced;
        state.led_output.last_error = last_error;
    }

    async fn set_led_output_offloaded_state(
        &self,
        hardware_led_count: u16,
        mut frame: Vec<LedColor>,
        source_project_id: Option<String>,
        program: &BuiltinLedProgram,
        last_error: Option<String>,
    ) {
        let normalized_count = hardware_led_count.max(1);
        let target_len = normalized_count as usize;
        if frame.len() > target_len {
            frame.truncate(target_len);
        } else if frame.len() < target_len {
            frame.resize(target_len, LedColor::rgb(0, 0, 0));
        }

        let mut state = self.state.write().await;
        state.led_output.hardware_led_count = normalized_count;
        state.led_output.frame = frame;
        state.led_output.source_project_id = source_project_id;
        state.led_output.execution_mode = LedExecutionMode::Offloaded;
        state.led_output.frame_is_estimated = true;
        state.led_output.offloaded = Some(LedOffloadedState {
            mode: program.mode,
            speed: program.speed,
            color_index: program.color_index,
            source_effect_id: program.source_effect_id.clone(),
        });
        state.led_output.physically_synced = false;
        state.led_output.last_error = last_error;
    }

    async fn run_boot_sequence(
        &self,
        oled: &mut OledPanel,
        spec: &PublishedDisplaySpec,
    ) -> Result<()> {
        if spec.boot_sequence.is_empty() {
            return Ok(());
        }

        for step in &spec.boot_sequence {
            {
                let mut state = self.state.write().await;
                state.active_page = RuntimePage::from_id(step.page_id.clone());
            }

            let frame = self.build_live_frame().await;
            oled.show_frame(&frame)?;

            self.publish_ui_snapshots().await;

            tokio::select! {
                _ = sleep(Duration::from_millis(step.duration_ms)) => {}
                _ = tokio::signal::ctrl_c() => {
                    info!("shutdown received during boot sequence");
                    oled.clear()?;
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    async fn build_custom_frame(
        &self,
        page_id: &str,
    ) -> Option<crate::display::render::PreviewFrame> {
        let (config, state, page) = {
            let config = self.config_store.read().await.config.clone();
            let state = self.state.read().await.clone();
            let page = self.page_store.read().await.get(page_id).cloned();
            (config, state, page)
        };

        let page = page?;
        let dynamic_context = dynamic_render_context_from_state_config(&state, &config);

        Some(render_page_definition_to_frame(
            config.display.width as usize,
            config.display.height as usize,
            &page,
            &state.metrics,
            &dynamic_context,
        ))
    }

    async fn build_live_frame(&self) -> crate::display::render::PreviewFrame {
        let (config, state) = {
            let config = self.config_store.read().await.config.clone();
            let state = self.state.read().await.clone();
            (config, state)
        };

        let page_id = state.active_page.page_id();
        if let Some(custom_frame) = self.build_custom_frame(page_id).await {
            custom_frame
        } else {
            crate::display::render::PreviewFrame::new(
                config.display.width as usize,
                config.display.height as usize,
            )
        }
    }

    async fn build_live_preview_snapshot(&self) -> PreviewSnapshot {
        let state = self.state.read().await.clone();
        let frame = self.build_live_frame().await;

        PreviewSnapshot {
            active_page: state.active_page.label(),
            frame,
        }
    }

    async fn publish_status_snapshot(&self) {
        let snapshot = {
            let state = self.state.read().await;
            UiSnapshot::from_state(&state)
        };

        let _ = self.event_tx.send(ServerEvent::Status(snapshot));
    }

    async fn publish_live_preview_snapshot(&self) {
        let snapshot = self.build_live_preview_snapshot().await;
        let _ = self.event_tx.send(ServerEvent::Preview(snapshot));
    }

    async fn publish_ui_snapshots(&self) {
        self.publish_status_snapshot().await;
        self.publish_live_preview_snapshot().await;
    }
}

async fn run_web_server(bind: String, ctx: WebContext) -> Result<()> {
    let addr: SocketAddr = bind
        .parse()
        .with_context(|| format!("failed to parse web bind address: {}", bind))?;

    let app = router(ctx);
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind web server to {}", addr))?;

    info!("web server listening on http://{}", addr);

    axum::serve(listener, app)
        .await
        .context("web server exited unexpectedly")?;

    Ok(())
}
