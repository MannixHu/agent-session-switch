use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const GITHUB_LATEST_RELEASE_API_URL: &str =
    "https://api.github.com/repos/MannixHu/agent-session-switch/releases/latest";
const GITHUB_API_ACCEPT: &str = "application/vnd.github+json";
const GITHUB_USER_AGENT: &str = "AgentSessionSwitchUpdater/1.0";
const UPDATE_TEMP_DIR_NAME: &str = "agent-session-switch-updates";

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
    pub release_notes: String,
    pub published_at: String,
    pub target_arch: String,
    pub asset_name: String,
    pub download_url: String,
    pub expected_sha256: String,
}

pub struct UpdateService;

impl UpdateService {
    fn build_http_client() -> Result<Client, String> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static(GITHUB_API_ACCEPT));
        headers.insert(USER_AGENT, HeaderValue::from_static(GITHUB_USER_AGENT));

        Client::builder()
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| format!("Failed to build HTTP client: {}", error))
    }

    /// Like [`build_http_client`] but with a download-sized budget: the
    /// installer can legitimately take minutes on slow links.
    fn build_download_client() -> Result<Client, String> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static(GITHUB_API_ACCEPT));
        headers.insert(USER_AGENT, HeaderValue::from_static(GITHUB_USER_AGENT));

        Client::builder()
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30 * 60))
            .build()
            .map_err(|error| format!("Failed to build HTTP client: {}", error))
    }

    fn normalize_version_tag(value: &str) -> Result<String, String> {
        let normalized = value.trim().trim_start_matches('v');
        if normalized.is_empty() {
            return Err("Version tag is empty".to_string());
        }

        Version::parse(normalized)
            .map_err(|error| format!("Invalid version tag {}: {}", value, error))?;

        Ok(normalized.to_string())
    }

    fn release_arch_suffix(arch: &str) -> Result<&'static str, String> {
        match arch.trim() {
            "aarch64" | "arm64" => Ok("arm64"),
            "x86_64" | "amd64" => Ok("x64"),
            other => Err(format!("Unsupported architecture: {}", other)),
        }
    }

    fn parse_checksums(content: &str) -> Vec<(String, String)> {
        content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return None;
                }

                let mut parts = trimmed.split_whitespace();
                let digest = parts.next()?.trim().to_lowercase();
                let file_name = parts.next()?.trim().to_string();

                Some((file_name, digest))
            })
            .collect()
    }

    fn parse_release(release_json: &str) -> Result<GithubRelease, String> {
        serde_json::from_str(release_json)
            .map_err(|error| format!("Failed to parse latest release payload: {}", error))
    }

    fn find_checksum_asset_url(release: &GithubRelease) -> Result<String, String> {
        release
            .assets
            .iter()
            .find(|asset| asset.name == "SHA256SUMS")
            .map(|asset| asset.browser_download_url.clone())
            .ok_or_else(|| "Latest release is missing SHA256SUMS".to_string())
    }

    fn update_temp_dir() -> Result<PathBuf, String> {
        let temp_dir = std::env::temp_dir().join(UPDATE_TEMP_DIR_NAME);
        fs::create_dir_all(&temp_dir).map_err(|error| {
            format!(
                "Failed to create update temp directory {}: {}",
                temp_dir.display(),
                error
            )
        })?;
        Ok(temp_dir)
    }

    pub fn resolve_release(
        current_version: &str,
        arch: &str,
        release_json: &str,
        checksums: &str,
    ) -> Result<UpdateCheckResult, String> {
        let normalized_current = Self::normalize_version_tag(current_version)?;
        let release = Self::parse_release(release_json)?;
        let normalized_latest = Self::normalize_version_tag(&release.tag_name)?;

        let current = Version::parse(&normalized_current).map_err(|error| {
            format!("Invalid current version {}: {}", normalized_current, error)
        })?;
        let latest = Version::parse(&normalized_latest)
            .map_err(|error| format!("Invalid latest version {}: {}", normalized_latest, error))?;

        let target_arch = Self::release_arch_suffix(arch)?.to_string();

        if latest <= current {
            return Ok(UpdateCheckResult {
                current_version: normalized_current,
                latest_version: normalized_latest,
                update_available: false,
                release_url: release.html_url,
                release_notes: release.body.clone().unwrap_or_default(),
                published_at: release.published_at,
                target_arch,
                asset_name: String::new(),
                download_url: String::new(),
                expected_sha256: String::new(),
            });
        }

        let expected_asset_name = format!(
            "AgentSessionSwitch_{}_{}.dmg",
            normalized_latest, target_arch
        );

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == expected_asset_name)
            .ok_or_else(|| {
                format!(
                    "Latest release is missing the {} installer for {}",
                    target_arch, normalized_latest
                )
            })?;

        let expected_sha256 = Self::parse_checksums(checksums)
            .into_iter()
            .find_map(|(file_name, digest)| {
                if file_name == expected_asset_name {
                    Some(digest)
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("Missing checksum for {}", expected_asset_name))?;

        Ok(UpdateCheckResult {
            current_version: normalized_current,
            latest_version: normalized_latest,
            update_available: true,
            release_url: release.html_url,
            release_notes: release.body.clone().unwrap_or_default(),
            published_at: release.published_at,
            target_arch,
            asset_name: expected_asset_name,
            download_url: asset.browser_download_url.clone(),
            expected_sha256,
        })
    }

    pub fn fetch_latest_release_info(
        current_version: &str,
        arch: &str,
    ) -> Result<UpdateCheckResult, String> {
        let client = Self::build_http_client()?;

        let release_json = client
            .get(GITHUB_LATEST_RELEASE_API_URL)
            .send()
            .map_err(|error| format!("Failed to fetch latest release: {}", error))?
            .error_for_status()
            .map_err(|error| format!("Latest release request failed: {}", error))?
            .text()
            .map_err(|error| format!("Failed to read latest release response: {}", error))?;

        let release = Self::parse_release(&release_json)?;
        let normalized_current = Self::normalize_version_tag(current_version)?;
        let normalized_latest = Self::normalize_version_tag(&release.tag_name)?;

        let current = Version::parse(&normalized_current).map_err(|error| {
            format!("Invalid current version {}: {}", normalized_current, error)
        })?;
        let latest = Version::parse(&normalized_latest)
            .map_err(|error| format!("Invalid latest version {}: {}", normalized_latest, error))?;
        let target_arch = Self::release_arch_suffix(arch)?.to_string();

        if latest <= current {
            return Ok(UpdateCheckResult {
                current_version: normalized_current,
                latest_version: normalized_latest,
                update_available: false,
                release_url: release.html_url,
                release_notes: release.body.clone().unwrap_or_default(),
                published_at: release.published_at,
                target_arch,
                asset_name: String::new(),
                download_url: String::new(),
                expected_sha256: String::new(),
            });
        }

        let checksum_url = Self::find_checksum_asset_url(&release)?;
        let checksums = client
            .get(checksum_url)
            .send()
            .map_err(|error| format!("Failed to fetch release checksums: {}", error))?
            .error_for_status()
            .map_err(|error| format!("Release checksums request failed: {}", error))?
            .text()
            .map_err(|error| format!("Failed to read release checksums response: {}", error))?;

        Self::resolve_release(&normalized_current, arch, &release_json, &checksums)
    }

    pub fn verify_sha256(path: &Path, expected_sha256: &str) -> Result<(), String> {
        let expected = expected_sha256.trim().to_lowercase();
        if expected.is_empty() {
            return Err("Expected checksum cannot be empty".to_string());
        }

        let mut file = fs::File::open(path).map_err(|error| {
            format!(
                "Failed to open downloaded file {}: {}",
                path.display(),
                error
            )
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 16 * 1024];

        loop {
            let bytes_read = file.read(&mut buffer).map_err(|error| {
                format!(
                    "Failed to read downloaded file {}: {}",
                    path.display(),
                    error
                )
            })?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let actual = format!("{:x}", hasher.finalize());
        if actual != expected {
            return Err(format!(
                "Downloaded file checksum mismatch for {}: expected {}, got {}",
                path.display(),
                expected,
                actual
            ));
        }

        Ok(())
    }

    pub fn download_update(
        download_url: &str,
        asset_name: &str,
        expected_sha256: &str,
    ) -> Result<PathBuf, String> {
        let normalized_url = download_url.trim();
        if normalized_url.is_empty() {
            return Err("Download URL cannot be empty".to_string());
        }

        let normalized_asset_name = asset_name.trim();
        if normalized_asset_name.is_empty() {
            return Err("Asset name cannot be empty".to_string());
        }

        let destination_dir = Self::update_temp_dir()?;
        let destination_path = destination_dir.join(normalized_asset_name);
        // Download to a `.part` staging file and only rename after the
        // checksum passes, so a crashed download can never be mistaken for
        // a complete installer.
        let partial_path = destination_dir.join(format!("{}.part", normalized_asset_name));
        let client = Self::build_download_client()?;
        let mut response = client
            .get(normalized_url)
            .send()
            .map_err(|error| format!("Failed to download update asset: {}", error))?
            .error_for_status()
            .map_err(|error| format!("Update asset request failed: {}", error))?;

        let mut output = fs::File::create(&partial_path).map_err(|error| {
            format!(
                "Failed to create downloaded installer {}: {}",
                partial_path.display(),
                error
            )
        })?;

        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let bytes_read = response
                .read(&mut buffer)
                .map_err(|error| format!("Failed to read update response body: {}", error))?;
            if bytes_read == 0 {
                break;
            }

            output
                .write_all(&buffer[..bytes_read])
                .map_err(|error| format!("Failed to write downloaded installer: {}", error))?;
        }

        output
            .flush()
            .map_err(|error| format!("Failed to flush downloaded installer: {}", error))?;

        if let Err(error) = Self::verify_sha256(&partial_path, expected_sha256) {
            let _ = fs::remove_file(&partial_path);
            return Err(error);
        }

        fs::rename(&partial_path, &destination_path).map_err(|error| {
            let _ = fs::remove_file(&partial_path);
            format!(
                "Failed to finalize downloaded installer {}: {}",
                destination_path.display(),
                error
            )
        })?;

        Ok(destination_path)
    }

    pub fn open_downloaded_installer(path: &Path) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            Command::new("open").arg(path).spawn().map_err(|error| {
                format!(
                    "Failed to open downloaded installer {}: {}",
                    path.display(),
                    error
                )
            })?;
            return Ok(());
        }

        #[allow(unreachable_code)]
        Err(format!(
            "Opening downloaded installers is only supported on macOS: {}",
            path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::UpdateService;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn latest_release_selects_arm64_asset_and_checksum() {
        let release_json = r###"{
          "tag_name": "v0.1.17",
          "html_url": "https://example.com/releases/v0.1.17",
          "body": "## Changelog\n\n| Commit | Description |\n| --- | --- |\n| `abc123` | feat: example |\n",
          "published_at": "2026-03-12T08:15:00Z",
          "assets": [
            {
              "name": "AgentSessionSwitch_0.1.17_arm64.dmg",
              "browser_download_url": "https://example.com/arm64.dmg"
            },
            {
              "name": "AgentSessionSwitch_0.1.17_x64.dmg",
              "browser_download_url": "https://example.com/x64.dmg"
            },
            {
              "name": "SHA256SUMS",
              "browser_download_url": "https://example.com/SHA256SUMS"
            }
          ]
        }"###;
        let checksums = "abc123  AgentSessionSwitch_0.1.17_arm64.dmg\nfff999  AgentSessionSwitch_0.1.17_x64.dmg\n";

        let result =
            UpdateService::resolve_release("0.1.16", "aarch64", release_json, checksums).unwrap();

        assert!(result.update_available);
        assert_eq!(result.current_version, "0.1.16");
        assert_eq!(result.latest_version, "0.1.17");
        assert_eq!(result.asset_name, "AgentSessionSwitch_0.1.17_arm64.dmg");
        assert_eq!(result.expected_sha256, "abc123");
        assert_eq!(result.release_url, "https://example.com/releases/v0.1.17");
        assert_eq!(result.published_at, "2026-03-12T08:15:00Z");
        assert!(result.release_notes.contains("## Changelog"));
    }

    #[test]
    fn latest_release_returns_up_to_date_when_versions_match() {
        let release_json = r#"{
          "tag_name": "v0.1.16",
          "assets": []
        }"#;

        let result = UpdateService::resolve_release("0.1.16", "aarch64", release_json, "").unwrap();

        assert!(!result.update_available);
        assert_eq!(result.latest_version, "0.1.16");
    }

    #[test]
    fn latest_release_accepts_null_body() {
        let release_json = r#"{
          "tag_name": "v0.1.17",
          "body": null,
          "assets": [
            {
              "name": "AgentSessionSwitch_0.1.17_arm64.dmg",
              "browser_download_url": "https://example.com/arm64.dmg"
            },
            {
              "name": "SHA256SUMS",
              "browser_download_url": "https://example.com/SHA256SUMS"
            }
          ]
        }"#;

        let result = UpdateService::resolve_release(
            "0.1.16",
            "aarch64",
            release_json,
            "abc123  AgentSessionSwitch_0.1.17_arm64.dmg\n",
        )
        .unwrap();

        assert_eq!(result.release_notes, "");
        assert!(result.update_available);
    }

    #[test]
    fn latest_release_rejects_missing_arch_asset() {
        let release_json = r#"{
          "tag_name": "v0.1.17",
          "assets": [
            {
              "name": "AgentSessionSwitch_0.1.17_x64.dmg",
              "browser_download_url": "https://example.com/x64.dmg"
            },
            {
              "name": "SHA256SUMS",
              "browser_download_url": "https://example.com/SHA256SUMS"
            }
          ]
        }"#;

        let error = UpdateService::resolve_release(
            "0.1.16",
            "aarch64",
            release_json,
            "fff999  AgentSessionSwitch_0.1.17_x64.dmg\n",
        )
        .unwrap_err();

        assert!(error.contains("arm64"));
    }

    #[test]
    fn verify_downloaded_file_accepts_matching_sha256() {
        let file_path = std::env::temp_dir().join(format!(
            "claude-session-switch-update-test-{}.dmg",
            Uuid::new_v4()
        ));
        fs::write(&file_path, b"hello world").unwrap();

        let result = UpdateService::verify_sha256(
            &file_path,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
        );

        let _ = fs::remove_file(&file_path);

        assert!(result.is_ok());
    }

    #[test]
    fn verify_downloaded_file_rejects_mismatched_sha256() {
        let file_path = std::env::temp_dir().join(format!(
            "claude-session-switch-update-test-{}.dmg",
            Uuid::new_v4()
        ));
        fs::write(&file_path, b"hello world").unwrap();

        let error = UpdateService::verify_sha256(&file_path, "deadbeef").unwrap_err();

        let _ = fs::remove_file(&file_path);

        assert!(error.contains("checksum"));
    }
}
