use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use rmcp::{
    ErrorData, Json, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use slugify::slugify;

use crate::templates::{
    ActionItems, IncidentStatus, IncidentTemplateData, TimelineEntry, append_timeline_entry,
    extract_patterns, extract_root_cause, extract_title, render_incident_markdown,
    update_incident_status,
};

#[derive(Debug, Clone)]
pub struct PostmortemServer {
    vault_root: PathBuf,
    incidents_dir: PathBuf,
    tool_router: ToolRouter<PostmortemServer>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for PostmortemServer {}

impl PostmortemServer {
    pub fn new(vault_root: impl Into<PathBuf>) -> Self {
        let vault_root = vault_root.into();
        let incidents_dir = vault_root.join("incidents");
        Self {
            vault_root,
            incidents_dir,
            tool_router: Self::tool_router(),
        }
    }

    pub fn ensure_storage(&self) -> Result<(), ErrorData> {
        fs::create_dir_all(&self.incidents_dir).map_err(|error| {
            ErrorData::internal_error(
                format!(
                    "failed to prepare incidents directory '{}' for vault '{}': {error}",
                    self.incidents_dir.display(),
                    self.vault_root.display()
                ),
                None,
            )
        })
    }

    fn incident_path(&self, incident_id: &str) -> PathBuf {
        self.incidents_dir.join(format!("{incident_id}.md"))
    }

    fn write_incident(&self, incident_id: &str, content: &str) -> Result<PathBuf, ErrorData> {
        let path = self.incident_path(incident_id);
        fs::write(&path, content).map_err(|error| {
            ErrorData::internal_error(format!("failed to write incident markdown: {error}"), None)
        })?;
        Ok(path)
    }

    fn read_incident(&self, incident_id: &str) -> Result<String, ErrorData> {
        let path = self.incident_path(incident_id);
        fs::read_to_string(&path).map_err(|_| ErrorData::invalid_params("incident not found", None))
    }

    fn list_incident_paths(&self) -> Result<Vec<PathBuf>, ErrorData> {
        let entries = fs::read_dir(&self.incidents_dir).map_err(|error| {
            ErrorData::internal_error(format!("failed to list incidents directory: {error}"), None)
        })?;

        let paths = entries
            .filter_map(|entry| entry.ok().map(|item| item.path()))
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
            .collect();
        Ok(paths)
    }

    fn tokenize(input: &str) -> HashSet<String> {
        input
            .split(|char: char| !char.is_ascii_alphanumeric())
            .filter(|part| !part.is_empty())
            .map(|part| part.to_ascii_lowercase())
            .collect()
    }

    fn parse_timestamp(value: Option<&str>) -> Result<DateTime<Utc>, ErrorData> {
        match value {
            Some(raw) => DateTime::parse_from_rfc3339(raw.trim())
                .map(|parsed| parsed.with_timezone(&Utc))
                .map_err(|error| {
                    ErrorData::invalid_params(
                        format!("invalid RFC3339 timestamp supplied: {error}"),
                        None,
                    )
                }),
            None => Ok(Utc::now()),
        }
    }

    fn require_non_empty<'a>(raw: &'a str, field: &str) -> Result<&'a str, ErrorData> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(ErrorData::invalid_params(
                format!("{field} cannot be empty"),
                None,
            ));
        }
        Ok(trimmed)
    }

    fn display_path(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempVault(PathBuf);

    impl TempVault {
        fn new() -> Self {
            let mut path = std::env::temp_dir();
            let suffix = Utc::now()
                .timestamp_nanos_opt()
                .unwrap_or_else(|| Utc::now().timestamp_micros() * 1_000);
            path.push(format!("lpm-mcp-test-{}-{suffix}", std::process::id()));
            fs::create_dir_all(&path).expect("temp vault directory to be created");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempVault {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn incident_lifecycle_tools_work_together() {
        let vault = TempVault::new();
        let server = PostmortemServer::new(vault.path());

        let Json(created) = server
            .create_incident(Parameters(CreateIncidentParams {
                title: "Search fan-out saturation".to_string(),
                root_cause: Some("Unbounded task fan-out exhausted workers.".to_string()),
                detect_actions: Some(vec!["Add p95 queue alert".to_string()]),
                mitigate_actions: Some(vec!["Throttle fan-out to 20".to_string()]),
                prevent_actions: Some(vec!["Stress tests in CI".to_string()]),
                patterns: Some(vec!["fan-out overload".to_string()]),
                initial_timeline: Some(vec!["Page received from monitoring".to_string()]),
            }))
            .await
            .expect("incident should be created");

        assert_eq!(created.status, IncidentStatus::Open);
        assert!(Path::new(&created.file_path).exists());

        let Json(added) = server
            .add_timeline_entry(Parameters(AddTimelineEntryParams {
                incident_id: created.incident_id.clone(),
                summary: "Mitigation patch deployed".to_string(),
                timestamp: Some("2026-05-01T11:00:00Z".to_string()),
            }))
            .await
            .expect("timeline entry should be added");
        assert!(added.entry.contains("Mitigation patch deployed"));

        let Json(similar) = server
            .search_similar_incidents(Parameters(SearchSimilarIncidentsParams {
                query: "fan-out overload".to_string(),
                limit: Some(10),
            }))
            .await
            .expect("similar incidents search should succeed");
        assert_eq!(similar.incidents.len(), 1);
        assert_eq!(similar.incidents[0].incident_id, created.incident_id);

        let Json(resolved) = server
            .resolve_incident(Parameters(ResolveIncidentParams {
                incident_id: created.incident_id.clone(),
                resolution_summary: Some("Hotfix rolled out and traffic recovered".to_string()),
                resolved_at: Some("2026-05-01T12:00:00Z".to_string()),
            }))
            .await
            .expect("incident should resolve");
        assert_eq!(resolved.status, IncidentStatus::Resolved);

        let final_markdown = fs::read_to_string(created.file_path).expect("incident file readable");
        assert!(final_markdown.contains("status: resolved"));
        assert!(final_markdown.contains("Resolved: Hotfix rolled out and traffic recovered"));
    }

    #[tokio::test]
    async fn search_ranks_incidents_by_overlap() {
        let vault = TempVault::new();
        let server = PostmortemServer::new(vault.path());

        let Json(first) = server
            .create_incident(Parameters(CreateIncidentParams {
                title: "Database lock storm".to_string(),
                root_cause: Some("Deadlock in checkout transaction".to_string()),
                detect_actions: None,
                mitigate_actions: None,
                prevent_actions: None,
                patterns: Some(vec!["database deadlock".to_string()]),
                initial_timeline: None,
            }))
            .await
            .expect("first incident created");
        let Json(second) = server
            .create_incident(Parameters(CreateIncidentParams {
                title: "CDN cache misses".to_string(),
                root_cause: Some("Region cache invalidation lag".to_string()),
                detect_actions: None,
                mitigate_actions: None,
                prevent_actions: None,
                patterns: Some(vec!["cache miss".to_string()]),
                initial_timeline: None,
            }))
            .await
            .expect("second incident created");

        let Json(result) = server
            .search_similar_incidents(Parameters(SearchSimilarIncidentsParams {
                query: "database deadlock checkout".to_string(),
                limit: Some(2),
            }))
            .await
            .expect("search succeeds");

        assert_eq!(result.incidents.len(), 1);
        assert_eq!(result.incidents[0].incident_id, first.incident_id);
        assert_ne!(result.incidents[0].incident_id, second.incident_id);
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateIncidentParams {
    pub title: String,
    pub root_cause: Option<String>,
    pub detect_actions: Option<Vec<String>>,
    pub mitigate_actions: Option<Vec<String>>,
    pub prevent_actions: Option<Vec<String>>,
    pub patterns: Option<Vec<String>>,
    pub initial_timeline: Option<Vec<String>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CreateIncidentResult {
    pub incident_id: String,
    pub file_path: String,
    pub status: IncidentStatus,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddTimelineEntryParams {
    pub incident_id: String,
    pub summary: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AddTimelineEntryResult {
    pub incident_id: String,
    pub file_path: String,
    pub entry: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchSimilarIncidentsParams {
    pub query: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SimilarIncident {
    pub incident_id: String,
    pub title: String,
    pub score: f32,
    pub matched_terms: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SearchSimilarIncidentsResult {
    pub query: String,
    pub incidents: Vec<SimilarIncident>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResolveIncidentParams {
    pub incident_id: String,
    pub resolution_summary: Option<String>,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ResolveIncidentResult {
    pub incident_id: String,
    pub status: IncidentStatus,
    pub resolved_at: String,
    pub file_path: String,
}

#[tool_router(router = tool_router)]
impl PostmortemServer {
    #[tool(description = "Create a new incident postmortem markdown file.")]
    pub async fn create_incident(
        &self,
        Parameters(params): Parameters<CreateIncidentParams>,
    ) -> Result<Json<CreateIncidentResult>, ErrorData> {
        self.ensure_storage()?;

        let title = Self::require_non_empty(&params.title, "incident title")?;

        let created_at = Utc::now();
        let base_slug = {
            let slug = slugify(title, "", "-", Some(64));
            if slug.is_empty() {
                "incident".to_string()
            } else {
                slug
            }
        };
        let timestamp_part = created_at.format("%Y%m%d%H%M%S").to_string();

        let mut sequence: u32 = 0;
        let incident_id = loop {
            let candidate = if sequence == 0 {
                format!("{base_slug}-{timestamp_part}")
            } else {
                format!("{base_slug}-{timestamp_part}-{sequence}")
            };
            if !self.incident_path(&candidate).exists() {
                break candidate;
            }
            sequence += 1;
        };

        let timeline_entries = params
            .initial_timeline
            .unwrap_or_default()
            .into_iter()
            .map(|summary| summary.trim().to_string())
            .filter(|summary| !summary.is_empty())
            .map(|summary| TimelineEntry {
                timestamp: created_at,
                summary,
            })
            .collect();

        let template = IncidentTemplateData {
            incident_id: incident_id.clone(),
            title: title.to_string(),
            status: IncidentStatus::Open,
            created_at,
            resolved_at: None,
            root_cause: params
                .root_cause
                .map(|root| root.trim().to_string())
                .filter(|root| !root.is_empty())
                .unwrap_or_else(|| "_TBD_".to_string()),
            timeline: timeline_entries,
            action_items: ActionItems {
                detect: params.detect_actions.unwrap_or_default(),
                mitigate: params.mitigate_actions.unwrap_or_default(),
                prevent: params.prevent_actions.unwrap_or_default(),
            },
            patterns: params.patterns.unwrap_or_default(),
        };
        let markdown = render_incident_markdown(&template);
        let file_path = self.write_incident(&incident_id, &markdown)?;

        Ok(Json(CreateIncidentResult {
            incident_id,
            file_path: Self::display_path(&file_path),
            status: IncidentStatus::Open,
        }))
    }

    #[tool(description = "Append a timeline entry to an incident.")]
    pub async fn add_timeline_entry(
        &self,
        Parameters(params): Parameters<AddTimelineEntryParams>,
    ) -> Result<Json<AddTimelineEntryResult>, ErrorData> {
        self.ensure_storage()?;
        let incident_id = Self::require_non_empty(&params.incident_id, "incident_id")?;
        let summary = Self::require_non_empty(&params.summary, "summary")?;

        let timestamp = Self::parse_timestamp(params.timestamp.as_deref())?;
        let entry = TimelineEntry {
            timestamp,
            summary: summary.to_string(),
        };

        let existing = self.read_incident(incident_id)?;
        let updated = append_timeline_entry(&existing, &entry);
        let file_path = self.write_incident(incident_id, &updated)?;

        Ok(Json(AddTimelineEntryResult {
            incident_id: incident_id.to_string(),
            file_path: Self::display_path(&file_path),
            entry: entry.markdown_line(),
        }))
    }

    #[tool(description = "Search similar incidents by lexical overlap.")]
    pub async fn search_similar_incidents(
        &self,
        Parameters(params): Parameters<SearchSimilarIncidentsParams>,
    ) -> Result<Json<SearchSimilarIncidentsResult>, ErrorData> {
        self.ensure_storage()?;
        let query = Self::require_non_empty(&params.query, "query")?;
        let query_tokens = Self::tokenize(query);

        let limit = params.limit.unwrap_or(5).clamp(1, 50);
        let mut incidents = Vec::new();

        for path in self.list_incident_paths()? {
            let markdown = fs::read_to_string(&path).map_err(|error| {
                ErrorData::internal_error(
                    format!("failed reading incident for similarity search: {error}"),
                    None,
                )
            })?;

            let title = extract_title(&markdown).unwrap_or_else(|| "Untitled incident".to_string());
            let root_cause = extract_root_cause(&markdown);
            let patterns = extract_patterns(&markdown).join(" ");
            let corpus = format!("{title} {root_cause} {patterns}");
            let tokens = Self::tokenize(&corpus);
            let matched_terms = query_tokens.intersection(&tokens).count();
            if matched_terms == 0 {
                continue;
            }

            let score = matched_terms as f32 / query_tokens.len() as f32;
            let incident_id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_string();

            incidents.push(SimilarIncident {
                incident_id,
                title,
                score,
                matched_terms,
            });
        }

        incidents.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then(right.matched_terms.cmp(&left.matched_terms))
                .then(left.title.cmp(&right.title))
        });
        incidents.truncate(limit);

        Ok(Json(SearchSimilarIncidentsResult {
            query: query.to_string(),
            incidents,
        }))
    }

    #[tool(description = "Resolve an incident and mark status as resolved.")]
    pub async fn resolve_incident(
        &self,
        Parameters(params): Parameters<ResolveIncidentParams>,
    ) -> Result<Json<ResolveIncidentResult>, ErrorData> {
        self.ensure_storage()?;
        let incident_id = Self::require_non_empty(&params.incident_id, "incident_id")?;
        let resolved_at = Self::parse_timestamp(params.resolved_at.as_deref())?;
        let existing = self.read_incident(incident_id)?;
        let mut updated =
            update_incident_status(&existing, IncidentStatus::Resolved, Some(resolved_at));

        if let Some(trimmed) = params
            .resolution_summary
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let resolution_entry = TimelineEntry {
                timestamp: resolved_at,
                summary: format!("Resolved: {trimmed}"),
            };
            updated = append_timeline_entry(&updated, &resolution_entry);
        }

        let file_path = self.write_incident(incident_id, &updated)?;
        Ok(Json(ResolveIncidentResult {
            incident_id: incident_id.to_string(),
            status: IncidentStatus::Resolved,
            resolved_at: resolved_at.to_rfc3339(),
            file_path: Self::display_path(&file_path),
        }))
    }
}
