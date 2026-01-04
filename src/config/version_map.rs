use crate::core::{ensure_dir_exists, FridaMgrError, HttpClient, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;
use tokio::time::{sleep, Duration};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionMapping {
    pub mappings: HashMap<String, VersionInfo>,
    pub aliases: HashMap<String, String>,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionInfo {
    pub tools: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objection: Option<String>,
    pub released: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Metadata {
    pub last_updated: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolsVersionResolution {
    pub tools_version: String,
    pub mapped_from_frida: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectionVersionResolution {
    pub objection_version: String,
    pub mapped_from_frida: String,
}

impl VersionMapping {
    pub fn builtin() -> Self {
        let mut mappings = HashMap::new();

        // Add version mappings (frida -> frida-tools)
        mappings.insert(
            "16.6.6".to_string(),
            VersionInfo {
                tools: "13.3.0".to_string(),
                objection: None,
                released: "2024-12-10".to_string(),
            },
        );
        mappings.insert(
            "16.5.2".to_string(),
            VersionInfo {
                tools: "13.2.2".to_string(),
                objection: None,
                released: "2024-11-15".to_string(),
            },
        );
        mappings.insert(
            "16.4.0".to_string(),
            VersionInfo {
                tools: "13.1.0".to_string(),
                objection: None,
                released: "2024-10-01".to_string(),
            },
        );
        mappings.insert(
            "16.1.4".to_string(),
            VersionInfo {
                tools: "12.2.1".to_string(),
                objection: None,
                released: "2024-06-15".to_string(),
            },
        );
        mappings.insert(
            "16.0.19".to_string(),
            VersionInfo {
                tools: "12.1.3".to_string(),
                objection: None,
                released: "2024-05-01".to_string(),
            },
        );
        mappings.insert(
            "15.2.2".to_string(),
            VersionInfo {
                tools: "12.0.4".to_string(),
                objection: None,
                released: "2023-12-20".to_string(),
            },
        );
        mappings.insert(
            "15.1.17".to_string(),
            VersionInfo {
                tools: "11.0.2".to_string(),
                objection: None,
                released: "2023-10-15".to_string(),
            },
        );

        let mut aliases = HashMap::new();
        aliases.insert("latest".to_string(), "16.6.6".to_string());
        aliases.insert("stable".to_string(), "16.4.0".to_string());
        aliases.insert("lts".to_string(), "15.2.2".to_string());

        Self {
            mappings,
            aliases,
            metadata: Metadata {
                last_updated: "2025-01-15".to_string(),
                source: "https://github.com/frida/frida/releases".to_string(),
            },
        }
    }

    pub async fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path).await?;
        Ok(toml::from_str(&content)?)
    }

    pub async fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            ensure_dir_exists(parent).await?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content).await?;
        Ok(())
    }

    pub async fn load_or_init(path: &Path) -> Result<Self> {
        if path.exists() {
            return Self::load(path).await;
        }

        let map = Self::builtin();
        map.save(path).await?;
        Ok(map)
    }

    pub fn resolve_alias(&self, version: &str) -> String {
        self.aliases
            .get(version)
            .cloned()
            .unwrap_or_else(|| version.to_string())
    }

    pub fn get_tools_version(&self, frida_version: &str) -> Option<String> {
        let resolved = self.resolve_alias(frida_version);
        self.mappings.get(&resolved).map(|info| info.tools.clone())
    }

    pub fn resolve_tools_version(&self, frida_version: &str) -> Option<ToolsVersionResolution> {
        let resolved = self.resolve_alias(frida_version);
        self.mappings
            .get(&resolved)
            .map(|info| ToolsVersionResolution {
                tools_version: info.tools.clone(),
                mapped_from_frida: resolved,
            })
    }

    pub fn get_objection_version(&self, frida_version: &str) -> Option<String> {
        let resolved = self.resolve_alias(frida_version);
        self.mappings
            .get(&resolved)
            .and_then(|info| info.objection.clone())
    }

    pub fn resolve_objection_version(
        &self,
        frida_version: &str,
    ) -> Option<ObjectionVersionResolution> {
        let resolved = self.resolve_alias(frida_version);
        self.mappings
            .get(&resolved)
            .and_then(|info| info.objection.clone())
            .map(|objection_version| ObjectionVersionResolution {
                objection_version,
                mapped_from_frida: resolved,
            })
    }

    pub fn list_versions(&self) -> Vec<String> {
        let mut versions: Vec<String> = self.mappings.keys().cloned().collect();
        versions.sort_by(
            |a, b| match (semver::Version::parse(a), semver::Version::parse(b)) {
                (Ok(a_ver), Ok(b_ver)) => b_ver.cmp(&a_ver),
                _ => b.cmp(a),
            },
        );
        versions
    }

    pub async fn build_from_github_releases(include_prerelease: bool) -> Result<Self> {
        let http = HttpClient::new();

        // Prefer Atom (no auth, 1 request), but in some environments it may return HTML.
        // Fallback to parsing the Releases HTML page (polite pagination).
        let frida = fetch_repo_releases(&http, "frida", "frida", include_prerelease).await?;

        // Prefer PyPI as the source-of-truth for installable Python package versions.
        // (GitHub tags don't always correspond 1:1 with PyPI releases, and dependencies can change.)
        //
        // If PyPI is unavailable, fall back to GitHub timestamps, but avoid pinning far-future
        // versions to reduce incompatibility risk.
        let (tools_by_date, tools_from_pypi) =
            match fetch_pypi_releases(&http, "frida-tools", include_prerelease).await {
                Ok(v) => (v, true),
                Err(_) => {
                    sleep(Duration::from_millis(200)).await;
                    let v = fetch_repo_releases(&http, "frida", "frida-tools", include_prerelease)
                        .await?
                        .into_iter()
                        .map(|r| PypiRelease {
                            version: r.version,
                            published_at: r.published_at,
                        })
                        .collect();
                    (v, false)
                }
            };

        // Objection versions should align with upstream GitHub releases (source of truth),
        // but we filter out versions that don't exist on PyPI to avoid non-installable pins.
        sleep(Duration::from_millis(200)).await;
        let mut objection_by_date =
            fetch_repo_releases(&http, "sensepost", "objection", include_prerelease).await?;
        objection_by_date.sort_by_key(|r| r.published_at);
        let mut objection_exists_cache: HashMap<String, Option<bool>> = HashMap::new();
        let mut tools_requires_cache: HashMap<String, Option<Vec<String>>> = HashMap::new();

        let mut mappings = HashMap::new();

        for fr in frida {
            let tools_release = if tools_from_pypi {
                select_compatible_tools_release_for_frida(
                    &http,
                    &tools_by_date,
                    &mut tools_requires_cache,
                    &fr.version,
                    fr.published_at,
                )
                .await?
            } else {
                select_release_near_future_or_previous(&tools_by_date, fr.published_at).cloned()
            };

            if let Some(tools_release) = tools_release {
                let objection_release = select_objection_release_for_frida(
                    &http,
                    &objection_by_date,
                    &mut objection_exists_cache,
                    fr.published_at,
                )
                .await;
                mappings.insert(
                    fr.version.to_string(),
                    VersionInfo {
                        tools: tools_release.version.to_string(),
                        objection: objection_release,
                        released: fr.published_at.date_naive().to_string(),
                    },
                );
            }
        }

        let aliases = build_default_aliases(&mappings);

        if mappings.is_empty() {
            return Err(FridaMgrError::Download(
                "Version mapping sync produced 0 entries; failed to parse releases data"
                    .to_string(),
            ));
        }

        Ok(Self {
            mappings,
            aliases,
            metadata: Metadata {
                last_updated: Utc::now().date_naive().to_string(),
                source: "https://github.com/frida/frida/releases.atom + https://pypi.org/pypi/frida-tools/json + https://github.com/sensepost/objection/releases.atom (filtered by PyPI availability)".to_string(),
            },
        })
    }
}

impl Default for VersionMapping {
    fn default() -> Self {
        Self::builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_mapping() {
        let mapping = VersionMapping::builtin();
        assert!(mapping.get_tools_version("16.6.6").is_some());
        assert_eq!(mapping.get_tools_version("16.6.6").unwrap(), "13.3.0");
    }

    #[test]
    fn test_alias_resolution() {
        let mapping = VersionMapping::builtin();
        assert_eq!(mapping.resolve_alias("latest"), "16.6.6");
        assert_eq!(mapping.get_tools_version("latest").unwrap(), "13.3.0");
    }

    #[tokio::test]
    async fn test_load_or_init_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("version-map.toml");

        let created = VersionMapping::load_or_init(&path).await.unwrap();
        assert!(path.exists());
        assert!(!created.mappings.is_empty());

        let loaded = VersionMapping::load_or_init(&path).await.unwrap();
        assert_eq!(created.mappings.len(), loaded.mappings.len());
    }

    #[test]
    fn test_find_nearest_by_date() {
        let tools = vec![
            NormalizedRelease {
                version: semver::Version::parse("1.0.0").unwrap(),
                published_at: DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            },
            NormalizedRelease {
                version: semver::Version::parse("1.1.0").unwrap(),
                published_at: DateTime::parse_from_rfc3339("2024-02-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            },
        ];

        let target = DateTime::parse_from_rfc3339("2024-01-20T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let nearest = find_nearest_by_date(&tools, target).unwrap();
        assert_eq!(nearest.version.to_string(), "1.1.0");
    }

    #[test]
    fn test_find_next_on_or_after_date() {
        let releases = vec![
            PypiRelease {
                version: semver::Version::parse("1.0.0").unwrap(),
                published_at: DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            },
            PypiRelease {
                version: semver::Version::parse("1.1.0").unwrap(),
                published_at: DateTime::parse_from_rfc3339("2024-02-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            },
        ];

        let t1 = DateTime::parse_from_rfc3339("2024-01-20T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = find_next_on_or_after_date(&releases, t1).unwrap();
        assert_eq!(next.version.to_string(), "1.1.0");

        let t2 = DateTime::parse_from_rfc3339("2024-02-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = find_next_on_or_after_date(&releases, t2).unwrap();
        assert_eq!(next.version.to_string(), "1.1.0");

        let t3 = DateTime::parse_from_rfc3339("2024-03-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(find_next_on_or_after_date(&releases, t3).is_none());
    }

    #[test]
    fn test_parse_frida_bounds_from_requires_dist() {
        let reqs = vec![
            "frida>=17.2.2".to_string(),
            "otherpkg>=1.0.0".to_string(),
            "frida<18.0.0".to_string(),
        ];

        let bounds = parse_frida_bounds_from_requires_dist(&reqs);
        assert_eq!(bounds.min_inclusive.as_ref().unwrap().to_string(), "17.2.2");
        assert_eq!(bounds.max_exclusive.as_ref().unwrap().to_string(), "18.0.0");
    }

    #[test]
    fn test_tools_compatible_with_frida_bounds() {
        let reqs = vec!["frida>=17.2.2".to_string(), "frida<18.0.0".to_string()];
        let frida_ok = semver::Version::parse("17.5.0").unwrap();
        let frida_too_low = semver::Version::parse("16.6.6").unwrap();
        let frida_too_high = semver::Version::parse("18.0.0").unwrap();

        assert!(tools_compatible_with_frida(Some(&reqs), &frida_ok));
        assert!(!tools_compatible_with_frida(Some(&reqs), &frida_too_low));
        assert!(!tools_compatible_with_frida(Some(&reqs), &frida_too_high));
    }

    #[test]
    fn test_parse_releases_html_minimal() {
        let html = r#"
<html><body>
<section>
  <relative-time datetime="2025-12-15T21:16:36Z">15 Dec</relative-time>
  <a href="/frida/frida/tree/17.5.2">17.5.2</a>
</section>
</body></html>
"#;

        let releases = parse_releases_html("frida", "frida", html, false).unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].version.to_string(), "17.5.2");
        assert_eq!(
            releases[0].published_at.to_rfc3339(),
            "2025-12-15T21:16:36+00:00"
        );
    }

    #[test]
    fn test_parse_atom_releases_handles_empty_link() {
        let xml = r#"
<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <updated>2025-12-16T00:20:21Z</updated>
  <entry>
    <updated>2025-12-16T00:20:36Z</updated>
    <link rel="alternate" type="text/html" href="https://github.com/frida/frida/releases/tag/17.5.2"/>
    <title>Frida 17.5.2</title>
  </entry>
</feed>
"#;

        let releases = parse_atom_releases("test://releases.atom", xml, false).unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].version.to_string(), "17.5.2");
        assert_eq!(
            releases[0].published_at.to_rfc3339(),
            "2025-12-16T00:20:36+00:00"
        );
    }

    #[test]
    fn test_parse_atom_releases_title_fallback() {
        let xml = r#"
<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <updated>2025-11-04T09:28:35Z</updated>
    <title>14.5.0: Require Frida &gt;= 17.5.0</title>
  </entry>
</feed>
"#;

        let releases = parse_atom_releases("test://releases.atom", xml, false).unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].version.to_string(), "14.5.0");
        assert_eq!(
            releases[0].published_at.to_rfc3339(),
            "2025-11-04T09:28:35+00:00"
        );
    }

    #[test]
    fn test_extract_next_releases_url() {
        let html = r#"
<html><head></head><body>
<div class="paginate-container">
  <div class="pagination">
    <a rel="next" href="/frida/frida/releases?page=2">Next</a>
  </div>
</div>
</body></html>
"#;
        assert_eq!(
            extract_next_releases_url(html).as_deref(),
            Some("/frida/frida/releases?page=2")
        );
        assert_eq!(
            normalize_github_href("/frida/frida/releases?page=2").unwrap(),
            "https://github.com/frida/frida/releases?page=2"
        );
    }
}

#[derive(Debug, Clone)]
struct NormalizedRelease {
    version: semver::Version,
    published_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct PypiRelease {
    version: semver::Version,
    published_at: DateTime<Utc>,
}

async fn fetch_atom_releases(
    http: &HttpClient,
    owner: &str,
    repo: &str,
    include_prerelease: bool,
) -> Result<Vec<NormalizedRelease>> {
    let url = format!("https://github.com/{}/{}/releases.atom", owner, repo);
    let xml = http.fetch_text(&url).await?;

    if looks_like_html(&xml) {
        return Err(FridaMgrError::Download(format!(
            "Expected Atom XML from {}, got HTML",
            url
        )));
    }

    parse_atom_releases(&url, &xml, include_prerelease)
}

fn parse_atom_releases(
    url: &str,
    xml: &str,
    include_prerelease: bool,
) -> Result<Vec<NormalizedRelease>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf: Vec<u8> = Vec::new();

    let mut releases: Vec<NormalizedRelease> = Vec::new();

    let mut in_entry = false;
    let mut current_text = String::new();
    let mut current_title: Option<String> = None;
    let mut current_published: Option<String> = None;
    let mut current_updated: Option<String> = None;
    let mut current_tag_from_link: Option<String> = None;

    enum Field {
        None,
        Title,
        Published,
        Updated,
    }
    let mut field = Field::None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"entry" => {
                    in_entry = true;
                    current_title = None;
                    current_published = None;
                    current_updated = None;
                    current_tag_from_link = None;
                    field = Field::None;
                }
                b"title" if in_entry => {
                    field = Field::Title;
                    current_text.clear();
                }
                b"published" if in_entry => {
                    field = Field::Published;
                    current_text.clear();
                }
                b"updated" if in_entry => {
                    field = Field::Updated;
                    current_text.clear();
                }
                b"link" if in_entry => {
                    // Prefer extracting the tag from the release URL (more stable than parsing title).
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() != b"href" {
                            continue;
                        }
                        if let Ok(href) = attr.unescape_value() {
                            let href = href.to_string();
                            if let Some((_, tag)) = href.rsplit_once("/tag/") {
                                current_tag_from_link = Some(tag.to_string());
                                break;
                            }
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.name().as_ref() {
                // GitHub Atom feeds typically use self-closing <link .../>.
                b"link" if in_entry => {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() != b"href" {
                            continue;
                        }
                        if let Ok(href) = attr.unescape_value() {
                            let href = href.to_string();
                            if let Some((_, tag)) = href.rsplit_once("/tag/") {
                                current_tag_from_link = Some(tag.to_string());
                                break;
                            }
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"title" if in_entry => {
                    current_title = Some(current_text.trim().to_string());
                    field = Field::None;
                }
                b"published" if in_entry => {
                    current_published = Some(current_text.trim().to_string());
                    field = Field::None;
                }
                b"updated" if in_entry => {
                    current_updated = Some(current_text.trim().to_string());
                    field = Field::None;
                }
                b"entry" => {
                    in_entry = false;

                    let title = current_title.as_deref().unwrap_or("");
                    let is_prerelease_title = title.to_ascii_lowercase().contains("pre-release")
                        || title.to_ascii_lowercase().contains("prerelease");

                    if !include_prerelease && is_prerelease_title {
                        continue;
                    }

                    let dt_str = current_published.as_deref().or(current_updated.as_deref());
                    let published_at =
                        match dt_str.and_then(|s| DateTime::parse_from_rfc3339(s).ok()) {
                            Some(v) => v.with_timezone(&Utc),
                            None => continue,
                        };

                    let tag = current_tag_from_link
                        .as_deref()
                        .or_else(|| extract_tag_from_title(title));
                    let tag = match tag {
                        Some(v) => v,
                        None => continue,
                    };
                    let tag = tag.trim();
                    let tag = tag.strip_prefix('v').unwrap_or(tag);

                    let version = match semver::Version::parse(tag) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    if !include_prerelease && !version.pre.is_empty() {
                        continue;
                    }

                    releases.push(NormalizedRelease {
                        version,
                        published_at,
                    });
                }
                _ => {}
            },
            Ok(Event::Text(e)) if in_entry => match field {
                Field::Title | Field::Published | Field::Updated => {
                    if let Ok(t) = e.unescape() {
                        current_text.push_str(&t);
                    }
                }
                Field::None => {}
            },
            Ok(_) => {}
            Err(e) => {
                return Err(FridaMgrError::Download(format!(
                    "Failed to parse Atom feed {}: {}",
                    url, e
                )));
            }
        }

        buf.clear();
    }

    // Deduplicate by version, keeping the latest published_at.
    releases.sort_by(|a, b| {
        a.version
            .cmp(&b.version)
            .then(a.published_at.cmp(&b.published_at))
    });
    let mut deduped: Vec<NormalizedRelease> = Vec::new();
    for r in releases {
        if let Some(last) = deduped.last_mut() {
            if last.version == r.version {
                if r.published_at > last.published_at {
                    *last = r;
                }
                continue;
            }
        }
        deduped.push(r);
    }

    Ok(deduped)
}

async fn fetch_repo_releases(
    http: &HttpClient,
    owner: &str,
    repo: &str,
    include_prerelease: bool,
) -> Result<Vec<NormalizedRelease>> {
    const MAX_HTML_PAGES: usize = 1000;
    let mut all: Vec<NormalizedRelease> = Vec::new();

    // Atom is cheap (1 request) but typically only includes the most recent entries.
    // We still try it first because some environments may block HTML pagination.
    if let Ok(atom) = fetch_atom_releases(http, owner, repo, include_prerelease).await {
        all.extend(atom);
    }

    // For a complete historical mapping we need the HTML pages (paginated).
    // If HTML fails but Atom succeeded, fall back to the partial Atom result.
    match fetch_html_releases(http, owner, repo, include_prerelease, MAX_HTML_PAGES).await {
        Ok(html) => all.extend(html),
        Err(e) if !all.is_empty() => return Ok(dedup_releases(all)),
        Err(e) => return Err(e),
    }

    Ok(dedup_releases(all))
}

async fn fetch_html_releases(
    http: &HttpClient,
    owner: &str,
    repo: &str,
    include_prerelease: bool,
    max_pages: usize,
) -> Result<Vec<NormalizedRelease>> {
    let mut all: Vec<NormalizedRelease> = Vec::new();
    let mut url = format!("https://github.com/{}/{}/releases", owner, repo);

    for _ in 0..max_pages {
        let html = http.fetch_text(&url).await?;
        if !looks_like_html(&html) {
            return Err(FridaMgrError::Download(format!(
                "Expected HTML from {}, got non-HTML response",
                url
            )));
        }

        let mut batch = parse_releases_html(owner, repo, &html, include_prerelease)?;
        if batch.is_empty() {
            break;
        }
        all.append(&mut batch);

        let next = extract_next_releases_url(&html)
            .map(|href| normalize_github_href(&href))
            .transpose()?;
        let Some(next) = next else {
            break;
        };
        if next == url {
            break;
        }
        url = next;

        // Be polite.
        sleep(Duration::from_millis(350)).await;
    }

    Ok(dedup_releases(all))
}

fn dedup_releases(mut releases: Vec<NormalizedRelease>) -> Vec<NormalizedRelease> {
    // Deduplicate by version, keeping the latest published_at.
    releases.sort_by(|a, b| {
        a.version
            .cmp(&b.version)
            .then(a.published_at.cmp(&b.published_at))
    });
    let mut deduped: Vec<NormalizedRelease> = Vec::new();
    for r in releases {
        if let Some(last) = deduped.last_mut() {
            if last.version == r.version {
                if r.published_at > last.published_at {
                    *last = r;
                }
                continue;
            }
        }
        deduped.push(r);
    }
    deduped
}

fn extract_next_releases_url(html: &str) -> Option<String> {
    // GitHub pagination commonly includes <a rel="next" href="...">Next</a>.
    // Some pages may also include <link rel="next" href="..."> in <head>.
    let a_re = Regex::new(r#"(?is)<a\b[^>]*\brel="next"[^>]*\bhref="([^"]+)""#).ok()?;
    if let Some(c) = a_re.captures(html).and_then(|c| c.get(1)) {
        return Some(c.as_str().to_string());
    }

    let link_re = Regex::new(r#"(?is)<link\b[^>]*\brel="next"[^>]*\bhref="([^"]+)""#).ok()?;
    link_re
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn normalize_github_href(href: &str) -> Result<String> {
    let href = href.trim();
    if href.starts_with("https://github.com/") {
        return Ok(href.to_string());
    }
    if href.starts_with('/') {
        return Ok(format!("https://github.com{}", href));
    }
    Err(FridaMgrError::Download(format!(
        "Unexpected next-page href format: {}",
        href
    )))
}

fn parse_releases_html(
    owner: &str,
    repo: &str,
    html: &str,
    include_prerelease: bool,
) -> Result<Vec<NormalizedRelease>> {
    let section_re = Regex::new(r"(?s)<section\b[^>]*>.*?</section>")
        .map_err(|e| FridaMgrError::Other(e.into()))?;
    let dt_re = Regex::new(r#"(?s)<relative-time\b[^>]*\bdatetime="([^"]+)""#)
        .map_err(|e| FridaMgrError::Other(e.into()))?;

    let tree_href_re = Regex::new(&format!(
        r#"/{}/{}/tree/([^"?#<>\s]+)"#,
        regex::escape(owner),
        regex::escape(repo)
    ))
    .map_err(|e| FridaMgrError::Other(e.into()))?;
    let release_href_re = Regex::new(&format!(
        r#"/{}/{}/releases/tag/([^"?#<>\s]+)"#,
        regex::escape(owner),
        regex::escape(repo)
    ))
    .map_err(|e| FridaMgrError::Other(e.into()))?;

    let mut out: Vec<NormalizedRelease> = Vec::new();

    for m in section_re.find_iter(html) {
        let section = m.as_str();

        let dt = match dt_re.captures(section).and_then(|c| c.get(1)) {
            Some(v) => v.as_str(),
            None => continue,
        };
        let published_at = match DateTime::parse_from_rfc3339(dt) {
            Ok(v) => v.with_timezone(&Utc),
            Err(_) => continue,
        };

        let tag = tree_href_re
            .captures(section)
            .and_then(|c| c.get(1))
            .or_else(|| release_href_re.captures(section).and_then(|c| c.get(1)))
            .map(|m| m.as_str());
        let tag = match tag {
            Some(v) => v,
            None => continue,
        };

        let tag = tag.trim().strip_prefix('v').unwrap_or(tag.trim());
        let version = match semver::Version::parse(tag) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if !include_prerelease && !version.pre.is_empty() {
            continue;
        }

        out.push(NormalizedRelease {
            version,
            published_at,
        });
    }

    Ok(out)
}

fn looks_like_html(s: &str) -> bool {
    let s = s.trim_start();
    s.starts_with("<!DOCTYPE html")
        || s.starts_with("<html")
        || s.contains("<html")
        || s.contains("</html>")
}

fn extract_tag_from_title(title: &str) -> Option<&str> {
    let title = title.trim();

    // Common formats:
    // - "Release 16.6.6"
    // - "Release v16.6.6"
    // - "Pre-release 16.6.6"
    // - "Pre-release v16.6.6"
    for prefix in ["Release ", "release ", "Pre-release ", "pre-release "] {
        if let Some(rest) = title.strip_prefix(prefix) {
            return Some(rest.trim());
        }
    }

    // GitHub releases Atom feed commonly uses titles like:
    // - "Frida 17.5.2"
    // - "14.5.0: Require Frida >= 17.5.0"
    for token in title.split_whitespace() {
        let token = token.trim().trim_end_matches(|c: char| {
            !c.is_ascii_alphanumeric() && c != '.' && c != '-' && c != '+'
        });
        let token = token.strip_prefix('v').unwrap_or(token);
        if semver::Version::parse(token).is_ok() {
            return Some(token);
        }
    }
    None
}

#[cfg(test)]
fn find_nearest_by_date<'a>(
    sorted_by_date: &'a [NormalizedRelease],
    target: DateTime<Utc>,
) -> Option<&'a NormalizedRelease> {
    if sorted_by_date.is_empty() {
        return None;
    }

    let idx = match sorted_by_date.binary_search_by_key(&target, |r| r.published_at) {
        Ok(i) => i,
        Err(i) => i,
    };

    let left = idx.checked_sub(1).and_then(|i| sorted_by_date.get(i));
    let right = sorted_by_date.get(idx);

    match (left, right) {
        (Some(a), Some(b)) => {
            let da = (target - a.published_at).num_seconds().abs();
            let db = (b.published_at - target).num_seconds().abs();
            if db < da {
                Some(b)
            } else if da < db {
                Some(a)
            } else {
                Some(b) // tie: prefer newer (right)
            }
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

#[cfg(test)]
fn find_next_on_or_after_date<'a>(
    sorted_by_date: &'a [PypiRelease],
    target: DateTime<Utc>,
) -> Option<&'a PypiRelease> {
    if sorted_by_date.is_empty() {
        return None;
    }

    let idx = match sorted_by_date.binary_search_by_key(&target, |r| r.published_at) {
        Ok(i) => i,
        Err(i) => i,
    };

    sorted_by_date.get(idx)
}

fn find_next_on_or_after_date_github<'a>(
    sorted_by_date: &'a [NormalizedRelease],
    target: DateTime<Utc>,
) -> Option<&'a NormalizedRelease> {
    if sorted_by_date.is_empty() {
        return None;
    }

    let idx = match sorted_by_date.binary_search_by_key(&target, |r| r.published_at) {
        Ok(i) => i,
        Err(i) => i,
    };

    sorted_by_date.get(idx)
}

async fn pypi_version_exists_cached(
    http: &HttpClient,
    cache: &mut HashMap<String, Option<bool>>,
    package: &str,
    version: &semver::Version,
) -> Option<bool> {
    let key = format!("{}=={}", package, version);
    if let Some(v) = cache.get(&key) {
        return *v;
    }

    let url = format!("https://pypi.org/pypi/{}/{}/json", package, version);
    let exists = match http.url_exists(&url).await {
        Ok(v) => Some(v),
        Err(_) => None,
    };
    cache.insert(key, exists);
    exists
}

async fn select_objection_release_for_frida(
    http: &HttpClient,
    objection_sorted_by_date: &[NormalizedRelease],
    exists_cache: &mut HashMap<String, Option<bool>>,
    frida_published_at: DateTime<Utc>,
) -> Option<String> {
    let idx = match objection_sorted_by_date
        .binary_search_by_key(&frida_published_at, |r| r.published_at)
    {
        Ok(i) => i,
        Err(i) => i,
    };

    // Prefer the first release on/after Frida that is known to exist on PyPI.
    for cand in objection_sorted_by_date.iter().skip(idx).take(30) {
        match pypi_version_exists_cached(http, exists_cache, "objection", &cand.version).await {
            Some(false) => continue,
            Some(true) | None => return Some(cand.version.to_string()),
        }
    }

    // Fallback: search backward if nothing suitable exists after.
    for cand in objection_sorted_by_date.iter().take(idx).rev().take(30) {
        match pypi_version_exists_cached(http, exists_cache, "objection", &cand.version).await {
            Some(false) => continue,
            Some(true) | None => return Some(cand.version.to_string()),
        }
    }

    // Last resort: keep mapping non-empty.
    find_next_on_or_after_date_github(objection_sorted_by_date, frida_published_at)
        .map(|r| r.version.to_string())
        .or_else(|| {
            objection_sorted_by_date
                .last()
                .map(|r| r.version.to_string())
        })
}

fn select_release_near_future_or_previous<'a>(
    sorted_by_date: &'a [PypiRelease],
    target: DateTime<Utc>,
) -> Option<&'a PypiRelease> {
    const MAX_FORWARD_LOOKAHEAD_DAYS: i64 = 21;

    if sorted_by_date.is_empty() {
        return None;
    }

    let idx = match sorted_by_date.binary_search_by_key(&target, |r| r.published_at) {
        Ok(i) => i,
        Err(i) => i,
    };

    let forward_deadline = target + ChronoDuration::days(MAX_FORWARD_LOOKAHEAD_DAYS);
    if let Some(next) = sorted_by_date.get(idx) {
        if next.published_at <= forward_deadline {
            return Some(next);
        }
    }

    idx.checked_sub(1)
        .and_then(|i| sorted_by_date.get(i))
        .or_else(|| sorted_by_date.first())
}

async fn fetch_pypi_releases(
    http: &HttpClient,
    package: &str,
    include_prerelease: bool,
) -> Result<Vec<PypiRelease>> {
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
    let index: PypiIndex = http.fetch_json(&url).await?;

    let mut out: Vec<PypiRelease> = Vec::new();
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

        out.push(PypiRelease {
            version: v,
            published_at,
        });
    }

    out.sort_by_key(|r| r.published_at);
    Ok(out)
}

async fn fetch_pypi_requires_dist(
    http: &HttpClient,
    package: &str,
    version: &semver::Version,
) -> Result<Option<Vec<String>>> {
    #[derive(Debug, Deserialize)]
    struct PypiVersionInfo {
        info: PypiInfo,
    }

    #[derive(Debug, Deserialize)]
    struct PypiInfo {
        requires_dist: Option<Vec<String>>,
    }

    let url = format!("https://pypi.org/pypi/{}/{}/json", package, version);
    let info: PypiVersionInfo = http.fetch_json(&url).await?;
    Ok(info.info.requires_dist)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VersionBounds {
    min_inclusive: Option<semver::Version>,
    max_exclusive: Option<semver::Version>,
}

fn parse_frida_bounds_from_requires_dist(requires_dist: &[String]) -> VersionBounds {
    let mut bounds = VersionBounds {
        min_inclusive: None,
        max_exclusive: None,
    };

    for raw in requires_dist {
        let requirement = raw.split(';').next().unwrap_or(raw).trim();
        let requirement = requirement
            .trim_start_matches("frida")
            .trim()
            .trim_start_matches(|c: char| c == '(' || c.is_whitespace())
            .trim_end_matches(')')
            .trim();

        // Only process lines that actually refer to frida.
        if !raw.trim_start().starts_with("frida") {
            continue;
        }

        for part in requirement
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let (op, ver) = if let Some(rest) = part.strip_prefix(">=") {
                (">=", rest)
            } else if let Some(rest) = part.strip_prefix("<=") {
                ("<=", rest)
            } else if let Some(rest) = part.strip_prefix("==") {
                ("==", rest)
            } else if let Some(rest) = part.strip_prefix("<") {
                ("<", rest)
            } else if let Some(rest) = part.strip_prefix(">") {
                (">", rest)
            } else {
                continue;
            };

            let ver = ver.trim().trim_start_matches('v');
            let Ok(v) = semver::Version::parse(ver) else {
                continue;
            };

            match op {
                ">=" => {
                    let replace = match bounds.min_inclusive.as_ref() {
                        None => true,
                        Some(cur) => v > *cur,
                    };
                    if replace {
                        bounds.min_inclusive = Some(v);
                    }
                }
                "<" => {
                    let replace = match bounds.max_exclusive.as_ref() {
                        None => true,
                        Some(cur) => v < *cur,
                    };
                    if replace {
                        bounds.max_exclusive = Some(v);
                    }
                }
                // Best-effort only; ignore the rest for now.
                _ => {}
            }
        }
    }

    bounds
}

fn tools_compatible_with_frida(
    tools_requires_dist: Option<&[String]>,
    frida: &semver::Version,
) -> bool {
    let Some(reqs) = tools_requires_dist else {
        return true;
    };
    let bounds = parse_frida_bounds_from_requires_dist(reqs);
    if let Some(min) = bounds.min_inclusive.as_ref() {
        if frida < min {
            return false;
        }
    }
    if let Some(max) = bounds.max_exclusive.as_ref() {
        if frida >= max {
            return false;
        }
    }
    true
}

async fn select_compatible_tools_release_for_frida(
    http: &HttpClient,
    tools_sorted_by_date: &[PypiRelease],
    requires_cache: &mut HashMap<String, Option<Vec<String>>>,
    frida_version: &semver::Version,
    frida_published_at: DateTime<Utc>,
) -> Result<Option<PypiRelease>> {
    const MAX_FORWARD_LOOKAHEAD_DAYS: i64 = 21;

    if tools_sorted_by_date.is_empty() {
        return Ok(None);
    }

    let idx =
        match tools_sorted_by_date.binary_search_by_key(&frida_published_at, |r| r.published_at) {
            Ok(i) => i,
            Err(i) => i,
        };

    let forward_deadline = frida_published_at + ChronoDuration::days(MAX_FORWARD_LOOKAHEAD_DAYS);
    let forward_candidates = tools_sorted_by_date
        .iter()
        .skip(idx)
        .take_while(|r| r.published_at <= forward_deadline);

    for cand in forward_candidates {
        let key = cand.version.to_string();
        let requires = match requires_cache.get(&key) {
            Some(v) => v.clone(),
            None => {
                let v = fetch_pypi_requires_dist(http, "frida-tools", &cand.version).await?;
                requires_cache.insert(key.clone(), v.clone());
                v
            }
        };

        if tools_compatible_with_frida(requires.as_deref(), frida_version) {
            return Ok(Some(cand.clone()));
        }
    }

    for cand in tools_sorted_by_date[..idx].iter().rev() {
        let key = cand.version.to_string();
        let requires = match requires_cache.get(&key) {
            Some(v) => v.clone(),
            None => {
                let v = fetch_pypi_requires_dist(http, "frida-tools", &cand.version).await?;
                requires_cache.insert(key.clone(), v.clone());
                v
            }
        };

        if tools_compatible_with_frida(requires.as_deref(), frida_version) {
            return Ok(Some(cand.clone()));
        }
    }

    // Last resort: pick the closest-by-time entry (may be incompatible, but avoids empty mappings).
    let fallback = tools_sorted_by_date
        .get(idx)
        .or_else(|| tools_sorted_by_date.last());
    Ok(fallback.cloned())
}

fn build_default_aliases(mappings: &HashMap<String, VersionInfo>) -> HashMap<String, String> {
    let mut parsed: Vec<semver::Version> = mappings
        .keys()
        .filter_map(|v| semver::Version::parse(v).ok())
        .collect();
    parsed.sort();

    let mut aliases = HashMap::new();
    if let Some(latest) = parsed.last() {
        aliases.insert("latest".to_string(), latest.to_string());
        aliases.insert("stable".to_string(), latest.to_string());

        let lts_major = latest.major.saturating_sub(1);
        if let Some(lts) = parsed.iter().rev().find(|v| v.major == lts_major) {
            aliases.insert("lts".to_string(), lts.to_string());
        }
    }

    aliases
}
