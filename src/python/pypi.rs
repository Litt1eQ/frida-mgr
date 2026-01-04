use crate::core::{HttpClient, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;

pub struct PypiClient {
    http: HttpClient,
}

impl PypiClient {
    pub fn new() -> Self {
        Self {
            http: HttpClient::new(),
        }
    }

    pub async fn requires_python(&self, package: &str, version: &str) -> Result<Option<String>> {
        #[derive(Debug, Deserialize)]
        struct PypiVersionInfo {
            info: PypiInfo,
        }

        #[derive(Debug, Deserialize)]
        struct PypiInfo {
            requires_python: Option<String>,
        }

        let url = format!("https://pypi.org/pypi/{}/{}/json", package, version);
        let info: PypiVersionInfo = self.http.fetch_json(&url).await?;
        Ok(info.info.requires_python)
    }

    pub async fn list_releases(
        &self,
        package: &str,
        include_prerelease: bool,
    ) -> Result<Vec<(semver::Version, DateTime<Utc>)>> {
        #[derive(Debug, Deserialize)]
        struct PypiIndex {
            releases: HashMap<String, Vec<PypiFile>>,
        }

        #[derive(Debug, Deserialize)]
        struct PypiFile {
            upload_time_iso_8601: Option<String>,
            upload_time: Option<String>,
            yanked: Option<bool>,
        }

        let url = format!("https://pypi.org/pypi/{}/json", package);
        let index: PypiIndex = self.http.fetch_json(&url).await?;

        let mut out: Vec<(semver::Version, DateTime<Utc>)> = Vec::new();
        for (version_str, files) in index.releases {
            let v = match semver::Version::parse(version_str.trim_start_matches('v')) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if !include_prerelease && !v.pre.is_empty() {
                continue;
            }

            let mut best: Option<DateTime<Utc>> = None;
            let mut any_non_yanked = false;
            for f in files {
                if f.yanked.unwrap_or(false) {
                    continue;
                }
                any_non_yanked = true;
                let dt_str = f
                    .upload_time_iso_8601
                    .as_deref()
                    .or(f.upload_time.as_deref());
                let Some(dt_str) = dt_str else {
                    continue;
                };
                let dt = match DateTime::parse_from_rfc3339(dt_str) {
                    Ok(v) => v.with_timezone(&Utc),
                    Err(_) => continue,
                };
                best = match best {
                    Some(current) if current <= dt => Some(current),
                    Some(_) => Some(dt),
                    None => Some(dt),
                };
            }
            if !any_non_yanked {
                continue;
            }
            let Some(published_at) = best else {
                continue;
            };
            out.push((v, published_at));
        }

        out.sort_by_key(|(_, dt)| *dt);
        Ok(out)
    }

    pub async fn select_first_compatible_on_or_after(
        &self,
        package: &str,
        after: DateTime<Utc>,
        python_version: &str,
    ) -> Result<Option<String>> {
        let releases = self.list_releases(package, false).await?;
        if releases.is_empty() {
            return Ok(None);
        }

        let idx = match releases.binary_search_by_key(&after, |(_, dt)| *dt) {
            Ok(i) => i,
            Err(i) => i,
        };

        // Search forward (closest in time after `after`).
        for (v, _) in releases.iter().skip(idx).take(50) {
            if let Ok(Some(req_py)) = self.requires_python(package, &v.to_string()).await {
                if !self.python_satisfies(&req_py, python_version) {
                    continue;
                }
            }
            return Ok(Some(v.to_string()));
        }

        // Fallback: search backward if nothing works (still ensures installability).
        for (v, _) in releases.iter().take(idx).rev().take(50) {
            if let Ok(Some(req_py)) = self.requires_python(package, &v.to_string()).await {
                if !self.python_satisfies(&req_py, python_version) {
                    continue;
                }
            }
            return Ok(Some(v.to_string()));
        }

        Ok(None)
    }

    pub fn python_satisfies(&self, requires_python: &str, python_version: &str) -> bool {
        python_satisfies(requires_python, python_version)
    }
}

impl Default for PypiClient {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_python_version(python_version: &str) -> Option<(u64, u64, u64)> {
    let s = python_version.trim();
    let s = s.strip_prefix('v').unwrap_or(s);
    let s = s
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<String>();
    let mut parts = s.split('.').filter(|p| !p.is_empty());
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    Some((major, minor, patch))
}

fn cmp_version(a: (u64, u64, u64), b: (u64, u64, u64)) -> std::cmp::Ordering {
    a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2))
}

pub fn python_satisfies(requires_python: &str, python_version: &str) -> bool {
    let py = match parse_python_version(python_version) {
        Some(v) => v,
        None => return true,
    };

    let spec = requires_python.trim();
    if spec.is_empty() {
        return true;
    }

    for raw in spec.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let (op, v_str) = if let Some(rest) = raw.strip_prefix(">=") {
            (">=", rest)
        } else if let Some(rest) = raw.strip_prefix("<=") {
            ("<=", rest)
        } else if let Some(rest) = raw.strip_prefix("==") {
            ("==", rest)
        } else if let Some(rest) = raw.strip_prefix("<") {
            ("<", rest)
        } else if let Some(rest) = raw.strip_prefix(">") {
            (">", rest)
        } else {
            // Unsupported (e.g. ~=, !=); best-effort: ignore.
            continue;
        };

        let v_str = v_str.trim();

        if op == "==" && v_str.ends_with(".*") {
            let prefix = v_str.trim_end_matches(".*");
            let Some((maj, min, _)) = parse_python_version(prefix) else {
                continue;
            };
            if py.0 != maj || py.1 != min {
                return false;
            }
            continue;
        }

        let Some(v) = parse_python_version(v_str) else {
            continue;
        };

        let ord = cmp_version(py, v);
        let ok = match op {
            ">=" => ord != std::cmp::Ordering::Less,
            ">" => ord == std::cmp::Ordering::Greater,
            "<=" => ord != std::cmp::Ordering::Greater,
            "<" => ord == std::cmp::Ordering::Less,
            "==" => ord == std::cmp::Ordering::Equal || (py.0 == v.0 && py.1 == v.1 && v.2 == 0),
            _ => true,
        };
        if !ok {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_requires_python_ge() {
        assert!(python_satisfies(">=3.11", "3.11.12"));
        assert!(!python_satisfies(">=3.14", "3.11.12"));
    }

    #[test]
    fn python_requires_python_range() {
        assert!(python_satisfies(">=3.8, <4", "3.11.12"));
        assert!(!python_satisfies(">=3.8, <3.11", "3.11.12"));
    }

    #[test]
    fn python_requires_python_wildcard() {
        assert!(python_satisfies("==3.11.*", "3.11.12"));
        assert!(!python_satisfies("==3.10.*", "3.11.12"));
    }

    #[test]
    fn pypi_parse_python_version_handles_suffix() {
        assert_eq!(parse_python_version("3.11.12"), Some((3, 11, 12)));
        assert_eq!(parse_python_version("3.11.12.final.0"), Some((3, 11, 12)));
    }
}
