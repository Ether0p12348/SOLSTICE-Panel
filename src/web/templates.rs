use askama::Template;

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {
    pub page_title: String,
    pub preview_css_width: usize,
    pub preview_css_height: usize,
    pub display_width: usize,
    pub display_height: usize,
    pub fan_auto_on_temp_c: u8,
    pub current_page: String,
    pub display_mode: String,
    pub rotation_interval_ms: u64,
    pub page_catalog_json: String,
    pub published_spec_json: String,
}

#[derive(Template)]
#[template(path = "studio.html")]
pub struct StudioTemplate {
    pub page_title: String,
    pub preview_css_width: usize,
    pub preview_css_height: usize,
    pub display_width: usize,
    pub display_height: usize,
    pub initial_catalog_key: String,
    pub page_catalog_json: String,
    pub published_spec_json: String,
}

#[derive(Template)]
#[template(path = "configuration.html")]
pub struct ConfigurationTemplate {
    pub page_title: String,
    pub config_schema_json: String,
    pub system_config_toml: String,
}

#[derive(Template)]
#[template(path = "led_studio.html")]
pub struct LedStudioTemplate {
    pub page_title: String,
    pub lab_bootstrap_json: String,
}

#[derive(Template)]
#[template(path = "process_explorer.html")]
pub struct ProcessExplorerTemplate {
    pub page_title: String,
}
