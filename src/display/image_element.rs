use crate::page_store::ImageMaskMode;
use base64::Engine;
use image::{DynamicImage, ImageBuffer, RgbaImage, imageops::FilterType};
use resvg::{tiny_skia, usvg};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    source: String,
    width: u32,
    height: u32,
    mask_mode: ImageMaskMode,
    threshold: u8,
}

#[derive(Debug, Clone)]
pub struct MonoMaskImage {
    pub width: u32,
    pub height: u32,
    pub mask: Vec<bool>,
}

static IMAGE_CACHE: OnceLock<Mutex<HashMap<CacheKey, Option<MonoMaskImage>>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<CacheKey, Option<MonoMaskImage>>> {
    IMAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn load_monochrome_mask(
    source: &str,
    width: u32,
    height: u32,
    mask_mode: ImageMaskMode,
    threshold: u8,
) -> Option<MonoMaskImage> {
    let source = source.trim();
    if source.is_empty() || width == 0 || height == 0 {
        return None;
    }

    let key = CacheKey {
        source: source.to_string(),
        width,
        height,
        mask_mode,
        threshold,
    };

    if let Ok(guard) = cache().lock() {
        if let Some(existing) = guard.get(&key) {
            return existing.clone();
        }
    }

    let created = build_mask(source, width, height, mask_mode, threshold);

    if let Ok(mut guard) = cache().lock() {
        guard.insert(key, created.clone());
    }

    created
}

fn build_mask(
    source: &str,
    width: u32,
    height: u32,
    mask_mode: ImageMaskMode,
    threshold: u8,
) -> Option<MonoMaskImage> {
    let (bytes, hint) = load_source_bytes(source)?;
    let rgba = if hint.mime_hint.as_deref() == Some("image/svg+xml")
        || hint.extension.as_deref() == Some("svg")
    {
        render_svg_to_rgba(&bytes, width, height)?
    } else {
        decode_bitmap_to_rgba(&bytes, width, height)?
    };

    let mask = rgba_to_mask(&rgba, mask_mode, threshold);
    Some(MonoMaskImage {
        width,
        height,
        mask,
    })
}

#[derive(Debug)]
struct SourceHint {
    mime_hint: Option<String>,
    extension: Option<String>,
}

fn load_source_bytes(source: &str) -> Option<(Vec<u8>, SourceHint)> {
    if source.starts_with("data:") {
        return parse_data_uri(source);
    }

    let path = resolve_source_path(source);
    let bytes = fs::read(&path).ok()?;
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase());

    Some((
        bytes,
        SourceHint {
            mime_hint: None,
            extension,
        },
    ))
}

fn resolve_source_path(source: &str) -> PathBuf {
    let source_path = Path::new(source);
    if source_path.is_absolute() {
        return source_path.to_path_buf();
    }
    PathBuf::from(source)
}

fn parse_data_uri(source: &str) -> Option<(Vec<u8>, SourceHint)> {
    let comma_idx = source.find(',')?;
    let (header, payload) = source.split_at(comma_idx);
    let payload = &payload[1..];

    let meta = header.strip_prefix("data:")?;
    let parts: Vec<&str> = meta.split(';').collect();
    let mime_hint = parts
        .first()
        .and_then(|m| (!m.is_empty()).then(|| (*m).to_string()));
    let is_base64 = parts.iter().any(|part| *part == "base64");

    let bytes = if is_base64 {
        base64::engine::general_purpose::STANDARD
            .decode(payload)
            .ok()?
    } else {
        return None;
    };

    let extension = match mime_hint.as_deref() {
        Some("image/svg+xml") => Some("svg".to_string()),
        Some("image/png") => Some("png".to_string()),
        Some("image/jpeg") => Some("jpg".to_string()),
        Some("image/jpg") => Some("jpg".to_string()),
        Some("image/webp") => Some("webp".to_string()),
        _ => None,
    };

    Some((
        bytes,
        SourceHint {
            mime_hint,
            extension,
        },
    ))
}

fn decode_bitmap_to_rgba(bytes: &[u8], width: u32, height: u32) -> Option<RgbaImage> {
    let image = image::load_from_memory(bytes).ok()?;
    Some(resize_to_rgba(image, width, height))
}

fn resize_to_rgba(image: DynamicImage, width: u32, height: u32) -> RgbaImage {
    image
        .resize_exact(width, height, FilterType::Nearest)
        .to_rgba8()
}

fn render_svg_to_rgba(bytes: &[u8], width: u32, height: u32) -> Option<RgbaImage> {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(bytes, &options).ok()?;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    let original_size = tree.size();
    let sx = width as f32 / original_size.width();
    let sy = height as f32 / original_size.height();
    let transform = tiny_skia::Transform::from_scale(sx, sy);

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    ImageBuffer::from_raw(width, height, pixmap.data().to_vec())
}

fn rgba_to_mask(image: &RgbaImage, mask_mode: ImageMaskMode, threshold: u8) -> Vec<bool> {
    let mut out = Vec::with_capacity((image.width() * image.height()) as usize);

    for pixel in image.pixels() {
        let [r, g, b, a] = pixel.0;
        let luma = ((u16::from(r) * 299 + u16::from(g) * 587 + u16::from(b) * 114) / 1000) as u8;

        let filled = match mask_mode {
            ImageMaskMode::Alpha => a >= threshold,
            ImageMaskMode::AlphaInverted => a < threshold,
            ImageMaskMode::LumaLight => a > 0 && luma >= threshold,
            ImageMaskMode::LumaDark => a > 0 && luma <= threshold,
        };

        out.push(filled);
    }

    out
}
