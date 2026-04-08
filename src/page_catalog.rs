use serde::Serialize;

use crate::page_store::{PageDefinition, PageStore};

#[derive(Debug, Clone, Serialize)]
pub struct CatalogPageEntry {
    pub key: String,
    pub page_id: String,
    pub display_name: String,
    pub editable: bool,
    pub deletable: bool,
    pub previewable: bool,
    pub preview_target: String,
    pub element_count: usize,
}

pub fn catalog_key_for_page_id(page_id: &str) -> String {
    page_id.to_string()
}

pub fn parse_catalog_key(key: &str) -> Option<String> {
    if let Some(rest) = key.strip_prefix("custom:") {
        let id = rest.trim();
        return (!id.is_empty()).then(|| id.to_string());
    }

    if let Some(rest) = key.strip_prefix("system:") {
        let id = rest.trim();
        return (!id.is_empty()).then(|| id.to_string());
    }

    let id = key.trim();
    (!id.is_empty()).then(|| id.to_string())
}

pub fn build_catalog(page_store: &PageStore) -> Vec<CatalogPageEntry> {
    page_store
        .pages
        .iter()
        .map(catalog_entry_for_page)
        .collect()
}

pub fn catalog_entry_for_page_id(
    page_store: &PageStore,
    page_id: &str,
) -> Option<CatalogPageEntry> {
    let page = page_store.get(page_id)?;
    Some(catalog_entry_for_page(page))
}

fn catalog_entry_for_page(page: &PageDefinition) -> CatalogPageEntry {
    let key = catalog_key_for_page_id(&page.id);
    CatalogPageEntry {
        key: key.clone(),
        page_id: page.id.clone(),
        display_name: page.name.clone(),
        editable: true,
        deletable: true,
        previewable: true,
        preview_target: key,
        element_count: page.elements.len(),
    }
}
