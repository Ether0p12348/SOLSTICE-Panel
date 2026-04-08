use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::RwLock;

use crate::{
    page_store::{PAGE_ID_BOOT, PAGE_ID_LIVE_INFO, canonical_page_id},
    state::{AppState, DisplayMode, RuntimePage},
};

pub type SharedPublishedStore = Arc<RwLock<PublishedStore>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootStep {
    #[serde(alias = "page_ref", deserialize_with = "deserialize_page_id")]
    pub page_id: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedDisplaySpec {
    pub boot_sequence: Vec<BootStep>,
    pub rotation_interval_ms: u64,
    #[serde(deserialize_with = "deserialize_page_id_list")]
    pub rotation_queue: Vec<String>,
}

impl PublishedDisplaySpec {
    pub fn normalize_canonical_ids(&mut self) -> bool {
        let mut changed = false;

        for step in &mut self.boot_sequence {
            let canonical = canonical_page_id(&step.page_id);
            if canonical != step.page_id {
                step.page_id = canonical;
                changed = true;
            }
        }

        for page_id in &mut self.rotation_queue {
            let canonical = canonical_page_id(page_id);
            if canonical != *page_id {
                *page_id = canonical;
                changed = true;
            }
        }

        changed
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.rotation_interval_ms >= 250,
            "rotation_interval_ms must be at least 250"
        );
        ensure!(
            !self.rotation_queue.is_empty(),
            "rotation_queue cannot be empty"
        );

        for (idx, step) in self.boot_sequence.iter().enumerate() {
            ensure!(
                step.duration_ms >= 100,
                "boot_sequence[{idx}].duration_ms must be at least 100"
            );
            ensure!(
                !step.page_id.trim().is_empty(),
                "boot_sequence[{idx}] references an empty page id"
            );
        }

        for (idx, page_id) in self.rotation_queue.iter().enumerate() {
            ensure!(
                !page_id.trim().is_empty(),
                "rotation_queue[{idx}] references an empty page id"
            );
        }

        Ok(())
    }

    pub fn runtime_queue(&self) -> Vec<RuntimePage> {
        self.rotation_queue
            .iter()
            .cloned()
            .map(RuntimePage::from_id)
            .collect()
    }

    pub fn apply_to_runtime_state(&self, state: &mut AppState) {
        state.rotation_interval_ms = self.rotation_interval_ms;
        state.rotation_queue = self.runtime_queue();
        state.rotation_index = 0;

        if let Some(first) = state.rotation_queue.first().cloned() {
            state.active_page = first;
        }

        state.display_mode = if state.rotation_queue.is_empty() {
            DisplayMode::Manual
        } else {
            DisplayMode::Rotating
        };
    }
}

#[derive(Debug, Clone)]
pub struct PublishedStore {
    pub path: PathBuf,
    pub spec: PublishedDisplaySpec,
}

impl PublishedStore {
    pub fn load_or_create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();

        if !path_buf.exists() {
            let spec = default_spec();
            let store = Self {
                path: path_buf,
                spec,
            };
            store.save()?;
            return Ok(store);
        }

        let raw = fs::read_to_string(&path_buf)
            .with_context(|| format!("failed to read published spec: {}", path_buf.display()))?;
        let needs_legacy_migration = raw.contains("\"kind\"") || raw.contains("\"page_ref\"");
        let mut spec: PublishedDisplaySpec =
            serde_json::from_str(&raw).context("failed to parse published spec JSON")?;
        let canonicalized = spec.normalize_canonical_ids();
        spec.validate()?;

        let store = Self {
            path: path_buf,
            spec,
        };

        if needs_legacy_migration || canonicalized {
            store.save()?;
        }

        Ok(store)
    }

    pub fn save(&self) -> Result<()> {
        self.spec.validate()?;
        let raw = serde_json::to_string_pretty(&self.spec)
            .context("failed to serialize published spec")?;
        fs::write(&self.path, raw)
            .with_context(|| format!("failed to write published spec: {}", self.path.display()))?;
        Ok(())
    }

    pub fn replace_spec(&mut self, spec: PublishedDisplaySpec) -> Result<()> {
        let mut spec = spec;
        spec.normalize_canonical_ids();
        spec.validate()?;
        self.spec = spec;
        self.save()
    }

    pub fn spec_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.spec).context("failed to render published spec JSON")
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PageIdWire {
    Id(String),
    Legacy(LegacyPageRef),
}

#[derive(Debug, Deserialize)]
struct LegacyPageRef {
    value: String,
}

fn deserialize_page_id<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = PageIdWire::deserialize(deserializer)?;
    Ok(canonical_page_id(&match value {
        PageIdWire::Id(id) => id,
        PageIdWire::Legacy(legacy) => legacy.value,
    }))
}

fn deserialize_page_id_list<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let values = Vec::<PageIdWire>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .map(|value| match value {
            PageIdWire::Id(id) => canonical_page_id(&id),
            PageIdWire::Legacy(legacy) => canonical_page_id(&legacy.value),
        })
        .collect())
}

fn default_spec() -> PublishedDisplaySpec {
    PublishedDisplaySpec {
        boot_sequence: vec![BootStep {
            page_id: PAGE_ID_BOOT.to_string(),
            duration_ms: 2000,
        }],
        rotation_interval_ms: 5000,
        rotation_queue: vec![PAGE_ID_LIVE_INFO.to_string()],
    }
}
