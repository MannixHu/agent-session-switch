use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub struct StorageService;

pub(crate) const DATA_DIR_OVERRIDE_ENV: &str = "CLOUD_CODE_SESSION_MANAGER_DATA_DIR";

impl StorageService {
    pub fn app_data_dir() -> PathBuf {
        let data_dir = Self::resolve_app_data_dir();

        if let Err(error) = fs::create_dir_all(&data_dir) {
            log::warn!(
                "Failed to ensure app data dir exists at {}: {}",
                data_dir.display(),
                error
            );
        }

        data_dir
    }

    fn resolve_app_data_dir() -> PathBuf {
        if let Some(override_dir) = std::env::var_os(DATA_DIR_OVERRIDE_ENV) {
            let normalized = override_dir.to_string_lossy().trim().to_string();
            if !normalized.is_empty() {
                return PathBuf::from(normalized);
            }
        }

        let base_dir = dirs::data_local_dir().unwrap_or_else(|| {
            let fallback = std::env::temp_dir();
            log::warn!(
                "Failed to resolve local data directory; using temp dir fallback: {}",
                fallback.display()
            );
            fallback
        });

        base_dir.join("CloudCodeSessionManager")
    }

    pub fn projects_file() -> PathBuf {
        Self::app_data_dir().join("projects.json")
    }

    pub fn preferences_file() -> PathBuf {
        Self::app_data_dir().join("preferences.json")
    }

    pub fn read<T: for<'de> Deserialize<'de>>(
        path: &Path,
    ) -> Result<T, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let data = serde_json::from_str(&content)?;
        Ok(data)
    }

    /// Atomically replace `path` with the serialized `data`: write to a temp
    /// file, fsync it, then rename over the target. The fsync ensures a
    /// crash cannot leave a renamed-but-empty file behind.
    pub fn write<T: Serialize>(path: &Path, data: &T) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(data)?;
        let temp_path = Self::temp_path_for(path);

        {
            use std::io::Write;
            let mut file = fs::File::create(&temp_path)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
        }

        match fs::rename(&temp_path, path) {
            Ok(()) => Ok(()),
            Err(rename_error) => {
                if path.exists() {
                    fs::remove_file(path)?;
                    fs::rename(&temp_path, path)?;
                    Ok(())
                } else {
                    let _ = fs::remove_file(&temp_path);
                    Err(Box::new(rename_error))
                }
            }
        }
    }

    fn temp_path_for(path: &Path) -> PathBuf {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("storage.json");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();

        path.with_file_name(format!("{}.{}.tmp", file_name, nanos))
    }
}

#[cfg(test)]
pub(crate) fn storage_test_env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

#[cfg(test)]
pub(crate) fn unique_test_data_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "ccsm-storage-test-{}-{}",
        name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default()
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        DATA_DIR_OVERRIDE_ENV, StorageService, storage_test_env_lock, unique_test_data_dir,
    };
    use std::fs;

    #[test]
    fn app_data_dir_prefers_explicit_override() {
        let _guard = storage_test_env_lock().lock().unwrap();
        let override_dir = unique_test_data_dir("override");

        unsafe {
            std::env::set_var(DATA_DIR_OVERRIDE_ENV, &override_dir);
        }

        let resolved = StorageService::app_data_dir();

        unsafe {
            std::env::remove_var(DATA_DIR_OVERRIDE_ENV);
        }

        assert_eq!(resolved, override_dir);
        assert!(resolved.is_dir());

        let _ = fs::remove_dir_all(resolved);
    }
}
