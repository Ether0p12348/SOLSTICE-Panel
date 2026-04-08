mod app;
mod config;
mod config_schema;
mod config_store;
mod display;
mod led;
mod metrics;
mod page_catalog;
mod page_store;
mod published_store;
mod state;
mod web;

use anyhow::Result;
use app::App;
use config::AppConfig;
use tracing_subscriber::{EnvFilter, fmt};

const CONFIG_PATH: &str = "config/solstice-panel.toml";
const PAGES_PATH: &str = "config/oled-pages.json";
const PUBLISHED_PATH: &str = "config/oled-published.json";
const LED_LAB_PATH: &str = "config/led-lab.json";

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let config = AppConfig::load_from_file(CONFIG_PATH)?;
    let app = App::new(
        CONFIG_PATH,
        PAGES_PATH,
        PUBLISHED_PATH,
        LED_LAB_PATH,
        config,
    )?;
    app.run().await
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
