use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSection {
    pub heading: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchReport {
    pub id: String,
    pub title: String,
    pub topic_ids: Vec<String>,
    pub sections: Vec<ReportSection>,
    pub format: String,
    pub output_path: Option<String>,
    pub generated_at: String,
}

impl ResearchReport {
    pub fn new(title: String, topic_ids: Vec<String>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title,
            topic_ids,
            sections: Vec::new(),
            format: "markdown".into(),
            output_path: None,
            generated_at: now,
        }
    }

    pub fn to_markdown(&self) -> String {
        let mut md = format!("# {}\n\n", self.title);
        for section in &self.sections {
            md.push_str(&format!(
                "## {}\n\n{}\n\n",
                section.heading, section.content
            ));
        }
        md
    }

    pub fn parse_sections(markdown: &str) -> Vec<ReportSection> {
        let mut sections = Vec::new();
        let mut current_heading: Option<String> = None;
        let mut current_lines = Vec::new();

        for line in markdown.lines() {
            if let Some(heading) = line.strip_prefix("## ") {
                if let Some(h) = current_heading.take() {
                    let content = current_lines.join("\n").trim().to_string();
                    sections.push(ReportSection {
                        heading: h,
                        content,
                    });
                    current_lines.clear();
                }
                current_heading = Some(heading.trim().to_string());
            } else if current_heading.is_some() {
                current_lines.push(line);
            }
        }

        if let Some(h) = current_heading {
            let content = current_lines.join("\n").trim().to_string();
            sections.push(ReportSection {
                heading: h,
                content,
            });
        }

        if sections.is_empty() {
            let trimmed = markdown.trim();
            if !trimmed.is_empty() {
                let content = trimmed
                    .lines()
                    .filter(|l| !l.starts_with("# "))
                    .collect::<Vec<_>>()
                    .join("\n")
                    .trim()
                    .to_string();
                let content = if content.is_empty() {
                    trimmed.to_string()
                } else {
                    content
                };
                sections.push(ReportSection {
                    heading: "Overview".into(),
                    content,
                });
            }
        }

        sections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_to_markdown() {
        let mut report = ResearchReport::new("Test Report".into(), vec!["t1".into()]);
        report.sections.push(ReportSection {
            heading: "Introduction".into(),
            content: "Some intro text.".into(),
        });
        report.sections.push(ReportSection {
            heading: "Findings".into(),
            content: "Key findings here.".into(),
        });
        let md = report.to_markdown();
        assert!(md.contains("# Test Report"));
        assert!(md.contains("## Introduction"));
        assert!(md.contains("Some intro text."));
    }

    #[test]
    fn parse_sections_roundtrip() {
        let mut report = ResearchReport::new("Test Report".into(), vec!["t1".into()]);
        report.sections.push(ReportSection {
            heading: "Introduction".into(),
            content: "Some intro text.".into(),
        });
        report.sections.push(ReportSection {
            heading: "Findings".into(),
            content: "Key findings here.".into(),
        });
        let md = report.to_markdown();
        let parsed = ResearchReport::parse_sections(&md);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].heading, "Introduction");
        assert_eq!(parsed[0].content, "Some intro text.");
        assert_eq!(parsed[1].heading, "Findings");
        assert_eq!(parsed[1].content, "Key findings here.");
    }
}
