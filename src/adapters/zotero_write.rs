//! Push paper tags into a running Zotero over its local HTTP API.
//!
//! The deliberately narrow write slice from the roadmap: one-way, tags only,
//! matched by normalized DOI, CLI-only (no MCP tool — an agent writing to a
//! user's library is a trust boundary we do not cross). Zotero's own GUI
//! confirmation dialog governs each write, so there is no key management
//! here; for batches, pick "Always Allow" in the dialog (writes otherwise
//! cost one confirmation each, capped at five per minute).
//!
//! Conflicts are never merged away: an item whose version moved since the
//! read is skipped and reported, because there is no UI to resolve a merge
//! and silently overwriting a user's bibliography is the failure mode this
//! slice exists to avoid.
//!
//! Docs: <https://www.zotero.org/support/dev/web_api/v3/local_api>

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use crate::adapters::bib_importer::ZoteroItem;
use crate::application::zotero_export::ZoteroTagSink;
use crate::error::{ResearchError, Result};

/// Default local-API base. `ZOTERO_BASE_URL` overrides it (non-default
/// installs, tests), matching [`crate::adapters::zotero_source`].
const DEFAULT_BASE_URL: &str = "http://localhost:23119/api/";

pub struct ZoteroWrite {
    base_url: String,
    client: reqwest::Client,
    /// Sent as `Zotero-Server-ID` when set (`ZOTERO_SERVER_ID` env). The
    /// local write path wants the instance's server ID; empty means the
    /// header is omitted and Zotero decides whether to accept the write.
    server_id: Option<String>,
}

impl ZoteroWrite {
    pub fn new() -> Self {
        let base_url =
            std::env::var("ZOTERO_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let server_id = std::env::var("ZOTERO_SERVER_ID")
            .ok()
            .filter(|s| !s.is_empty());
        // The write path PATCHes item data to the configured host; a
        // non-loopback target would send library metadata off-machine, so
        // say so loudly rather than silently trusting the env.
        let host = base_url
            .split("://")
            .nth(1)
            .unwrap_or("")
            .split('/')
            .next()
            .unwrap_or("");
        if !host.is_empty()
            && host != "localhost"
            && !host.starts_with("127.")
            && !host.starts_with("[::1]")
        {
            eprintln!(
                "warning: ZOTERO_BASE_URL points at {host} — Zotero writes (and item data) will be sent there"
            );
        }
        Self::with_base_url(base_url, server_id)
    }

    pub fn with_base_url(base_url: String, server_id: Option<String>) -> Self {
        let base_url = if base_url.ends_with('/') {
            base_url
        } else {
            format!("{base_url}/")
        };
        Self {
            base_url,
            client: reqwest::Client::new(),
            server_id,
        }
    }

    /// Write one merged tag set. Returns `false` when the item changed under
    /// us (version conflict): the caller reports it, nothing is merged.
    pub async fn patch_tags(&self, key: &str, version: i64, tags: &[String]) -> Result<bool> {
        let mut req = self
            .client
            .patch(format!("{}users/0/items/{key}", self.base_url))
            .header("If-Unmodified-Since-Version", version.to_string())
            .json(&tags_payload(tags));
        if let Some(id) = &self.server_id {
            req = req.header("Zotero-Server-ID", id);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ResearchError::Source(format!("Zotero write failed: {e}")))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(true);
        }
        if status == reqwest::StatusCode::PRECONDITION_FAILED
            || status == reqwest::StatusCode::CONFLICT
        {
            return Ok(false);
        }
        Err(status_error(status))
    }
}

impl Default for ZoteroWrite {
    fn default() -> Self {
        Self::new()
    }
}

/// The HTTP sink the exporter drives; see [`crate::application::zotero_export`].
#[async_trait]
impl ZoteroTagSink for ZoteroWrite {
    async fn library(&self) -> Result<Vec<RemoteItem>> {
        ZoteroWrite::library(self).await
    }
    async fn write_tags(&self, key: &str, version: i64, tags: &[String]) -> Result<bool> {
        ZoteroWrite::patch_tags(self, key, version, tags).await
    }
}

/// One item as the write path sees it.
#[derive(Clone)]
pub struct RemoteItem {
    pub key: String,
    pub version: i64,
    /// Normalized on the way in, so matching mirrors the import dedupe.
    pub doi: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Deserialize)]
struct ApiItem {
    key: String,
    version: i64,
    data: ZoteroItem,
}

/// Zotero caps a single page at 100 items; `library` pages through the whole
/// user library the same way the read source does.
const PAGE_SIZE: usize = 100;

impl ZoteroWrite {
    /// The whole library, paged: every item with its key, version, DOI, and
    /// current tags.
    pub async fn library(&self) -> Result<Vec<RemoteItem>> {
        let mut items = Vec::new();
        let mut start = 0;
        loop {
            let resp = self
                .client
                .get(format!("{}users/0/items", self.base_url))
                .query(&[
                    ("format", "json".to_string()),
                    ("qmode", "everything".to_string()),
                    ("start", start.to_string()),
                    ("limit", PAGE_SIZE.to_string()),
                ])
                .send()
                .await
                .map_err(|e| {
                    if e.is_connect() {
                        ResearchError::Source(format!(
                            "could not reach Zotero at {} (is Zotero running?): {e}",
                            self.base_url
                        ))
                    } else {
                        ResearchError::Source(format!("Zotero request failed: {e}"))
                    }
                })?;
            let status = resp.status();
            if !status.is_success() {
                return Err(status_error(status));
            }
            let body = resp
                .text()
                .await
                .map_err(|e| ResearchError::Source(format!("Zotero response read failed: {e}")))?;
            let page: Vec<ApiItem> = serde_json::from_str(&body).map_err(|e| {
                ResearchError::Source(format!("Zotero local API response parse failed: {e}"))
            })?;
            let got = page.len();
            items.extend(page.into_iter().map(|i| RemoteItem {
                key: i.key,
                version: i.version,
                doi: i.data.normalized_doi(),
                tags: i.data.tag_strings(),
            }));
            if got < PAGE_SIZE {
                break;
            }
            start += got;
        }
        Ok(items)
    }
}

/// Union of the Zotero item's tags and the library's tags: existing order
/// first (Zotero's own organization is not reordered), new tags appended in
/// library order, trimmed, deduplicated. Pure so the merge policy is unit
/// tested in isolation from HTTP.
pub fn merge_tags(existing: &[String], extra: &[String]) -> Vec<String> {
    let mut merged: Vec<String> = Vec::with_capacity(existing.len() + extra.len());
    for tag in existing.iter().chain(extra.iter()) {
        let tag = tag.trim();
        if !tag.is_empty() && !merged.iter().any(|m| m == tag) {
            merged.push(tag.to_string());
        }
    }
    merged
}

/// PATCH body for a tags-only update: Zotero's Web-API-v3 patch semantics
/// apply only the fields present, so title/creators/etc. are untouched.
pub fn tags_payload(tags: &[String]) -> serde_json::Value {
    json!({ "tags": tags.iter().map(|t| json!({ "tag": t })).collect::<Vec<_>>() })
}

/// Turn an HTTP status into an error naming the fix, mirroring the read
/// source's 403 message.
fn status_error(status: reqwest::StatusCode) -> ResearchError {
    if status == reqwest::StatusCode::FORBIDDEN {
        return ResearchError::Source(
            "Zotero returned HTTP 403 — enable \"Allow other applications on \
             this computer to communicate with Zotero\" in Zotero settings"
                .to_string(),
        );
    }
    if status == reqwest::StatusCode::METHOD_NOT_ALLOWED {
        return ResearchError::Source(
            "Zotero rejected the write (HTTP 405) — pushing tags needs Zotero \
             10 or newer; older versions serve a read-only local API"
                .to_string(),
        );
    }
    ResearchError::Source(format!("Zotero local API returned HTTP {status}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_existing_order_then_appends_new() {
        let existing = vec!["quantum sensing".into(), "diamond".into()];
        let extra = vec![
            "to-read".into(),
            "quantum sensing".into(),
            "  diamond  ".into(),
        ];
        assert_eq!(
            merge_tags(&existing, &extra),
            vec!["quantum sensing", "diamond", "to-read"]
        );
    }

    #[test]
    fn merge_drops_empty_and_whitespace_tags() {
        let merged = merge_tags(&[], &["".into(), "  ".into(), "real".into()]);
        assert_eq!(merged, vec!["real"]);
    }

    #[test]
    fn merge_with_no_overlap_preserves_both_sides() {
        let existing = vec!["a".into()];
        let extra = vec!["b".into()];
        assert_eq!(merge_tags(&existing, &extra), vec!["a", "b"]);
    }

    #[test]
    fn payload_is_patch_shaped_and_only_tags() {
        let body = tags_payload(&["x".into(), "y".into()]);
        let tags = body["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0]["tag"], "x");
        // Patch semantics: no other field may ride along and clobber Zotero.
        assert!(body.as_object().unwrap().len() == 1);
    }

    #[test]
    fn base_url_gets_trailing_slash() {
        let w = ZoteroWrite::with_base_url("http://localhost:9999/api".into(), None);
        assert_eq!(w.base_url, "http://localhost:9999/api/");
    }

    #[test]
    fn forbidden_status_names_the_preference() {
        let err = status_error(reqwest::StatusCode::FORBIDDEN).to_string();
        assert!(err.contains("Allow other applications"), "got: {err}");
    }
}
