use std::{
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;

pub(crate) const RELEASES_LATEST_URL: &str =
    "https://github.com/luodaoyi/grok-bridge-rs/releases/latest";
const GITHUB_API_LATEST: &str =
    "https://api.github.com/repos/luodaoyi/grok-bridge-rs/releases/latest";
pub(crate) const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const HTTP_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct VersionStatus {
    pub(crate) current: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) latest: Option<String>,
    pub(crate) update_available: bool,
    pub(crate) release_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) checked_at_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LatestRelease {
    version: String,
    release_url: String,
}

#[derive(Debug)]
struct CacheInner {
    latest: Option<String>,
    release_url: String,
    checked_at_ms: Option<u64>,
}

impl Default for CacheInner {
    fn default() -> Self {
        Self {
            latest: None,
            release_url: RELEASES_LATEST_URL.to_owned(),
            checked_at_ms: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct VersionChecker {
    current: String,
    inner: Mutex<CacheInner>,
}

impl VersionChecker {
    pub(crate) fn new() -> Self {
        Self {
            current: env!("CARGO_PKG_VERSION").to_owned(),
            inner: Mutex::new(CacheInner::default()),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_current(current: impl Into<String>) -> Self {
        Self {
            current: current.into(),
            inner: Mutex::new(CacheInner::default()),
        }
    }

    pub(crate) fn status(&self) -> VersionStatus {
        let guard = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let latest = guard.latest.clone();
        let update_available = latest
            .as_deref()
            .is_some_and(|latest| is_outdated(&self.current, latest));
        VersionStatus {
            current: self.current.clone(),
            latest,
            update_available,
            release_url: guard.release_url.clone(),
            checked_at_ms: guard.checked_at_ms,
        }
    }

    pub(crate) fn refresh(&self) {
        match fetch_latest_release() {
            Ok(release) => {
                let mut guard = self.inner.lock().unwrap_or_else(|error| error.into_inner());
                guard.latest = Some(release.version);
                guard.release_url = release.release_url;
                guard.checked_at_ms = Some(now_millis());
            }
            Err(error) => {
                eprintln!("grok-bridge server: version check failed: {error:#}");
                let mut guard = self.inner.lock().unwrap_or_else(|error| error.into_inner());
                guard.checked_at_ms = Some(now_millis());
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn seed_latest(&self, version: impl Into<String>, release_url: impl Into<String>) {
        let mut guard = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        guard.latest = Some(version.into());
        guard.release_url = release_url.into();
        guard.checked_at_ms = Some(1);
    }
}

pub(crate) fn normalize_tag(tag: &str) -> String {
    tag.trim().trim_start_matches(['v', 'V']).trim().to_owned()
}

pub(crate) fn parse_semver(version: &str) -> Option<(u64, u64, u64)> {
    let version = normalize_tag(version);
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch_part = parts.next()?;
    let patch = patch_part
        .split(|ch: char| !ch.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub(crate) fn is_outdated(current: &str, latest: &str) -> bool {
    match (parse_semver(current), parse_semver(latest)) {
        (Some(current), Some(latest)) => latest > current,
        _ => normalize_tag(latest) != normalize_tag(current),
    }
}

fn fetch_latest_release() -> Result<LatestRelease> {
    if env_disabled() {
        bail!("version check disabled by GROK_BRIDGE_DISABLE_UPDATE_CHECK");
    }
    let response = ureq::get(GITHUB_API_LATEST)
        .set("User-Agent", "grok-bridge")
        .set("Accept", "application/vnd.github+json")
        .timeout(HTTP_TIMEOUT)
        .call()
        .context("GitHub latest release request failed")?;
    let body: serde_json::Value = response
        .into_json()
        .context("GitHub latest release response is not valid JSON")?;
    parse_latest_release_json(&body)
}

fn env_disabled() -> bool {
    matches!(
        std::env::var("GROK_BRIDGE_DISABLE_UPDATE_CHECK").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

pub(crate) fn parse_latest_release_json(body: &serde_json::Value) -> Result<LatestRelease> {
    let tag = body
        .get("tag_name")
        .and_then(|value| value.as_str())
        .context("GitHub release JSON is missing tag_name")?;
    let version = normalize_tag(tag);
    if version.is_empty() {
        bail!("GitHub release tag_name is empty");
    }
    let release_url = body
        .get("html_url")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(RELEASES_LATEST_URL)
        .to_owned();
    Ok(LatestRelease {
        version,
        release_url,
    })
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_compares_semver_tags() {
        assert_eq!(normalize_tag("v0.6.2"), "0.6.2");
        assert_eq!(parse_semver("v1.2.3"), Some((1, 2, 3)));
        assert!(is_outdated("0.6.1", "0.6.2"));
        assert!(is_outdated("0.6.1", "v0.7.0"));
        assert!(!is_outdated("0.6.2", "0.6.2"));
        assert!(!is_outdated("0.6.2", "0.6.1"));
        assert!(!is_outdated("v0.6.2", "0.6.2"));
    }

    #[test]
    fn parses_github_latest_release_json() {
        let body = serde_json::json!({
            "tag_name": "v0.9.1",
            "html_url": "https://github.com/luodaoyi/grok-bridge-rs/releases/tag/v0.9.1"
        });
        let release = parse_latest_release_json(&body).unwrap();
        assert_eq!(release.version, "0.9.1");
        assert_eq!(
            release.release_url,
            "https://github.com/luodaoyi/grok-bridge-rs/releases/tag/v0.9.1"
        );
    }

    #[test]
    fn rejects_invalid_github_json() {
        assert!(parse_latest_release_json(&serde_json::json!({})).is_err());
        assert!(parse_latest_release_json(&serde_json::json!({ "tag_name": "" })).is_err());
    }

    #[test]
    fn status_reports_update_when_seeded_newer() {
        let checker = VersionChecker::with_current("0.6.1");
        assert!(!checker.status().update_available);
        checker.seed_latest(
            "0.6.2",
            "https://github.com/luodaoyi/grok-bridge-rs/releases/tag/v0.6.2",
        );
        let status = checker.status();
        assert!(status.update_available);
        assert_eq!(status.current, "0.6.1");
        assert_eq!(status.latest.as_deref(), Some("0.6.2"));
        assert!(status.release_url.contains("v0.6.2"));
    }

    #[test]
    fn status_serializes_with_stable_fields() {
        let checker = VersionChecker::with_current("0.6.1");
        let value = serde_json::to_value(checker.status()).unwrap();
        assert_eq!(value["current"], "0.6.1");
        assert_eq!(value["update_available"], false);
        assert_eq!(value["release_url"], RELEASES_LATEST_URL);
        assert!(value.get("latest").is_none());
    }
}
