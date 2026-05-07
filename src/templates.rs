use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const ROOT_CAUSE_HEADING: &str = "## Root Cause";
const ROOT_CAUSE_SECTION: &str = "\n## Root Cause\n";
const TIMELINE_SECTION: &str = "## Timeline";
const ACTION_ITEMS_SECTION: &str = "## Action Items";
const PATTERNS_SECTION: &str = "## Patterns";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    Open,
    Resolved,
}

impl IncidentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub timestamp: DateTime<Utc>,
    pub summary: String,
}

impl TimelineEntry {
    pub fn markdown_line(&self) -> String {
        format!("- {} — {}", self.timestamp.to_rfc3339(), self.summary)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionItems {
    pub detect: Vec<String>,
    pub mitigate: Vec<String>,
    pub prevent: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentTemplateData {
    pub incident_id: String,
    pub title: String,
    pub status: IncidentStatus,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub root_cause: String,
    pub timeline: Vec<TimelineEntry>,
    pub action_items: ActionItems,
    pub patterns: Vec<String>,
}

pub fn status_emoji(status: IncidentStatus) -> &'static str {
    match status {
        IncidentStatus::Open => "🟠",
        IncidentStatus::Resolved => "🟢",
    }
}

pub fn render_incident_markdown(data: &IncidentTemplateData) -> String {
    let timeline_block = render_bulleted(data.timeline.iter().map(TimelineEntry::markdown_line));
    let detect_block = render_bulleted(data.action_items.detect.iter().map(String::as_str));
    let mitigate_block = render_bulleted(data.action_items.mitigate.iter().map(String::as_str));
    let prevent_block = render_bulleted(data.action_items.prevent.iter().map(String::as_str));
    let patterns_block = render_bulleted(data.patterns.iter().map(String::as_str));
    let resolved_at = data
        .resolved_at
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "null".to_string());

    let mut markdown = String::new();
    markdown.push_str("---\n");
    markdown.push_str(&format!("incident_id: {}\n", data.incident_id));
    markdown.push_str(&format!("status: {}\n", data.status.as_str()));
    markdown.push_str(&format!("created_at: {}\n", data.created_at.to_rfc3339()));
    markdown.push_str(&format!("resolved_at: {resolved_at}\n"));
    markdown.push_str("---\n");
    markdown.push_str(&format!(
        "# {} {}\n\n",
        status_emoji(data.status),
        data.title
    ));

    markdown.push_str(TIMELINE_SECTION);
    markdown.push('\n');
    markdown.push_str(&timeline_block);
    markdown.push_str("\n\n");

    markdown.push_str(ROOT_CAUSE_HEADING);
    markdown.push('\n');
    markdown.push_str(data.root_cause.trim());
    markdown.push_str("\n\n");

    markdown.push_str(ACTION_ITEMS_SECTION);
    markdown.push('\n');
    markdown.push_str(&render_subsection("Detect", &detect_block));
    markdown.push_str(&render_subsection("Mitigate", &mitigate_block));
    markdown.push_str(&render_subsection("Prevent", &prevent_block));
    markdown.push('\n');

    markdown.push_str(PATTERNS_SECTION);
    markdown.push('\n');
    markdown.push_str(&patterns_block);
    markdown.push('\n');

    markdown
}

pub fn append_timeline_entry(markdown: &str, entry: &TimelineEntry) -> String {
    let insert_line = format!("{}\n", entry.markdown_line());
    if let Some(idx) = markdown.find(ROOT_CAUSE_SECTION) {
        let before = markdown[..idx].trim_end_matches('\n');
        let after = &markdown[idx..];
        format!("{before}\n{insert_line}{after}")
    } else {
        format!("{}\n{insert_line}", markdown.trim_end())
    }
}

pub fn update_incident_status(
    markdown: &str,
    status: IncidentStatus,
    resolved_at: Option<DateTime<Utc>>,
) -> String {
    let mut output = Vec::new();
    let resolved_at_value = resolved_at
        .map(|ts| ts.to_rfc3339())
        .unwrap_or_else(|| "null".to_string());

    for line in markdown.lines() {
        if line.starts_with("status: ") {
            output.push(format!("status: {}", status.as_str()));
            continue;
        }
        if line.starts_with("resolved_at: ") {
            output.push(format!("resolved_at: {resolved_at_value}"));
            continue;
        }
        if let Some(title) = parse_h1_title(line) {
            output.push(format!("# {} {}", status_emoji(status), title));
            continue;
        }
        output.push(line.to_string());
    }

    format!("{}\n", output.join("\n"))
}

pub fn extract_title(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .find_map(parse_h1_title)
        .map(ToOwned::to_owned)
}

pub fn extract_root_cause(markdown: &str) -> String {
    extract_section(markdown, "Root Cause").unwrap_or_default()
}

pub fn extract_patterns(markdown: &str) -> Vec<String> {
    extract_section(markdown, "Patterns")
        .map(|raw| {
            raw.lines()
                .filter_map(|line| line.trim().strip_prefix("- ").map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn extract_section(markdown: &str, section: &str) -> Option<String> {
    let marker = format!("## {section}\n");
    let start = markdown.find(&marker)?;
    let content_start = start + marker.len();
    let remainder = &markdown[content_start..];
    let end = remainder.find("\n## ").unwrap_or(remainder.len());
    Some(remainder[..end].trim().to_string())
}

fn render_bulleted<I, S>(lines: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut rendered = String::new();
    let mut has_lines = false;

    for line in lines {
        let line = line.as_ref();
        if has_lines {
            rendered.push('\n');
        } else {
            has_lines = true;
        }

        if line.trim_start().starts_with("- ") {
            rendered.push_str(line);
        } else {
            rendered.push_str("- ");
            rendered.push_str(line);
        }
    }

    if has_lines {
        rendered
    } else {
        "- _None yet_".to_string()
    }
}

fn render_subsection(name: &str, content: &str) -> String {
    format!("### {name}\n{content}\n")
}

fn parse_h1_title(line: &str) -> Option<&str> {
    if !line.starts_with("# ") {
        return None;
    }
    line.trim_start_matches("# ")
        .split_once(' ')
        .map(|(_, rest)| rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_incident() -> IncidentTemplateData {
        IncidentTemplateData {
            incident_id: "incident-001".to_string(),
            title: "Search service degraded under peak traffic".to_string(),
            status: IncidentStatus::Open,
            created_at: DateTime::parse_from_rfc3339("2026-05-01T10:00:00Z")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            resolved_at: None,
            root_cause: "Thread pool saturation after unbounded fan-out.".to_string(),
            timeline: vec![TimelineEntry {
                timestamp: DateTime::parse_from_rfc3339("2026-05-01T10:05:00Z")
                    .expect("valid timestamp")
                    .with_timezone(&Utc),
                summary: "First alert fired in EU region.".to_string(),
            }],
            action_items: ActionItems {
                detect: vec!["Add queue depth alert with p95 guardrails.".to_string()],
                mitigate: vec!["Cap concurrent fan-out to 20 requests.".to_string()],
                prevent: vec!["Load test fan-out scenarios in CI.".to_string()],
            },
            patterns: vec![
                "Fan-out overload".to_string(),
                "Insufficient backpressure".to_string(),
            ],
        }
    }

    #[test]
    fn render_incident_has_required_sections() {
        let markdown = render_incident_markdown(&sample_incident());
        assert!(markdown.contains("# 🟠 Search service degraded under peak traffic"));
        assert!(markdown.contains("## Timeline"));
        assert!(markdown.contains("## Root Cause"));
        assert!(markdown.contains("## Action Items"));
        assert!(markdown.contains("### Detect"));
        assert!(markdown.contains("### Mitigate"));
        assert!(markdown.contains("### Prevent"));
        assert!(markdown.contains("## Patterns"));
    }

    #[test]
    fn timeline_entry_is_appended_before_root_cause_section() {
        let base = render_incident_markdown(&sample_incident());
        let appended = append_timeline_entry(
            &base,
            &TimelineEntry {
                timestamp: DateTime::parse_from_rfc3339("2026-05-01T10:15:00Z")
                    .expect("valid timestamp")
                    .with_timezone(&Utc),
                summary: "Traffic shaping enabled.".to_string(),
            },
        );
        let marker = "- 2026-05-01T10:15:00+00:00 — Traffic shaping enabled.";
        assert!(appended.contains(marker));
        assert!(
            appended.find(marker).expect("line present")
                < appended.find("## Root Cause").expect("root cause present")
        );
    }

    #[test]
    fn status_updates_header_and_frontmatter() {
        let base = render_incident_markdown(&sample_incident());
        let resolved = update_incident_status(
            &base,
            IncidentStatus::Resolved,
            Some(
                DateTime::parse_from_rfc3339("2026-05-01T11:00:00Z")
                    .expect("valid timestamp")
                    .with_timezone(&Utc),
            ),
        );

        assert!(resolved.contains("status: resolved"));
        assert!(resolved.contains("resolved_at: 2026-05-01T11:00:00+00:00"));
        assert!(resolved.contains("# 🟢 Search service degraded under peak traffic"));
    }

    #[test]
    fn section_extractors_return_expected_values() {
        let markdown = render_incident_markdown(&sample_incident());
        assert_eq!(
            extract_title(&markdown).as_deref(),
            Some("Search service degraded under peak traffic")
        );
        assert_eq!(
            extract_root_cause(&markdown),
            "Thread pool saturation after unbounded fan-out."
        );
        assert_eq!(
            extract_patterns(&markdown),
            vec![
                "Fan-out overload".to_string(),
                "Insufficient backpressure".to_string()
            ]
        );
    }
}
