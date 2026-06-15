//! Icon helpers for ThinUI
//!
//! Loads bundled icons from res/icons directory and caches them.

use cosmic::widget::icon;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub(crate) static ICON_CACHE: OnceLock<Mutex<IconCache>> = OnceLock::new();

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct IconCacheKey {
    name: String,
    size: u16,
}

#[derive(Debug)]
pub struct IconCacheEntry {
    pub handle: icon::Handle,
    pub _bytes: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct IconCache {
    cache: HashMap<IconCacheKey, IconCacheEntry>,
    bundled_icons: std::collections::HashSet<String>,
}

impl Default for IconCache {
    fn default() -> Self {
        Self::new()
    }
}

impl IconCache {
    pub fn new() -> Self {
        let mut bundled_icons = std::collections::HashSet::new();

        let icons_dir = get_bundled_icons_path();
        tracing::info!("Loading icons from: {}", icons_dir.display());
        if let Ok(entries) = fs::read_dir(&icons_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Some(stripped) = name.strip_suffix(".svg") {
                        bundled_icons.insert(stripped.to_string());
                        tracing::debug!("Found bundled icon: {}", stripped);
                    }
                }
            }
        } else {
            tracing::warn!("Failed to read icons directory: {}", icons_dir.display());
        }
        tracing::info!("Loaded {} bundled icons", bundled_icons.len());
        Self {
            cache: HashMap::new(),
            bundled_icons,
        }
    }

    fn get_icon(&mut self, name: &str, size: u16) -> icon::Icon {
        let key = IconCacheKey {
            name: name.to_string(),
            size,
        };
        if let Some(entry) = self.cache.get(&key) {
            return icon::icon(entry.handle.clone()).size(size);
        }
        let (handle, bytes) = if self.bundled_icons.contains(name) {
            let path = get_bundled_icons_path().join(format!("{}.svg", name));
            let data = fs::read(&path)
                .unwrap_or_else(|e| {
                    tracing::error!(path = %path.display(), error = %e, "Failed to read bundled icon");
                    Vec::new() // Return empty vec as fallback
                });
            let handle = icon::from_svg_bytes(data.clone()).symbolic(true);
            (handle, Some(data))
        } else {
            tracing::debug!("Icon '{}' not found in bundled icons, using system icon", name);
            (icon::from_name(name).size(size).handle(), None)
        };
        self.cache.insert(
            key.clone(),
            IconCacheEntry {
                handle: handle.clone(),
                _bytes: bytes,
            },
        );
        icon::icon(handle).size(size)
    }
}

pub fn get_icon(name: &str, size: u16) -> icon::Icon {
    let icon_cache = match ICON_CACHE.get() {
        Some(cache) => cache,
        None => {
            tracing::error!("Icon cache not initialized, using fallback icon");
            return icon::from_name(name).size(size).icon();
        }
    };
    
    let mut icon_cache = match icon_cache.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("Icon cache lock was poisoned, recovering");
            poisoned.into_inner()
        }
    };
    
    icon_cache.get_icon(name, size)
}

fn get_bundled_icons_path() -> PathBuf {
    // Check if we're running in Flatpak (installed location)
    let flatpak_path = PathBuf::from("/app/res/icons");
    if flatpak_path.exists() {
        tracing::debug!("Using Flatpak icon path: {}", flatpak_path.display());
        return flatpak_path;
    }
    
    // Development mode - thin UI is in luna_thin_ui/, icons are in ../res/icons
    // CARGO_MANIFEST_DIR points to luna_thin_ui/, so we need to go up one level
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    tracing::debug!("CARGO_MANIFEST_DIR: {}", manifest_dir.display());
    
    // Try going up one level (from luna_thin_ui/ to workspace root)
    if let Some(parent) = manifest_dir.parent() {
        let dev_path = parent.join("res/icons");
        if dev_path.exists() {
            tracing::debug!("Using development icon path: {}", dev_path.display());
            return dev_path;
        }
        tracing::warn!("Development icon path does not exist: {}", dev_path.display());
    }
    
    // Fallback: try workspace root (if CARGO_MANIFEST_DIR is workspace root)
    let fallback_path = manifest_dir.join("res/icons");
    if fallback_path.exists() {
        tracing::debug!("Using fallback icon path: {}", fallback_path.display());
        return fallback_path;
    }
    
    tracing::warn!("No valid icon path found, using fallback: {}", fallback_path.display());
    fallback_path
}

pub fn get_handle(name: &str, size: u16) -> icon::Handle {
    let icon_cache = match ICON_CACHE.get() {
        Some(cache) => cache,
        None => {
            tracing::error!("Icon cache not initialized, using fallback icon handle");
            return icon::from_name(name).size(size).handle();
        }
    };
    
    let mut icon_cache = match icon_cache.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("Icon cache lock was poisoned, recovering");
            poisoned.into_inner()
        }
    };
    
    let key = IconCacheKey {
        name: name.to_string(),
        size,
    };
    if let Some(entry) = icon_cache.cache.get(&key) {
        return entry.handle.clone();
    }
    let (handle, bytes) = if icon_cache.bundled_icons.contains(name) {
        let path = get_bundled_icons_path().join(format!("{}.svg", name));
        let data = match fs::read(&path) {
            Ok(data) => data,
            Err(e) => {
                tracing::error!(
                    icon_name = name,
                    path = %path.display(),
                    error = %e,
                    "Failed to read bundled icon, using fallback"
                );
                // Return fallback handle
                return icon::from_name(name).size(size).handle();
            }
        };
        let handle = icon::from_svg_bytes(data.clone()).symbolic(true);
        (handle, Some(data))
    } else {
        tracing::debug!("Icon '{}' not found in bundled icons, using system icon", name);
        (icon::from_name(name).size(size).handle(), None)
    };
    icon_cache.cache.insert(
        key,
        IconCacheEntry {
            handle: handle.clone(),
            _bytes: bytes,
        },
    );
    handle
}




