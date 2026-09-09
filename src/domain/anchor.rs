//! Locating a position inside a stored paper body.
//!
//! Bodies carry two kinds of inline marker: `## Section` headings written by
//! the PDF ingest, and `<!-- page N -->` page boundaries. Resolving an offset
//! to the nearest preceding marker of each kind turns a search hit into
//! evidence a reader can find again.
//!
//! Section is the more durable anchor: a preprint's page 7 is not the
//! published version's page 7, but "Methods" is "Methods" in every rendering.
//! Page is reported alongside it because it is what people ask for.

use serde::{Deserialize, Serialize};

use crate::adapters::pdf_source::PAGE_MARKER_PREFIX;

/// Where a span of body text sits in the document it came from. Both fields
/// are absent for bodies stored before markers existed, and `page` is absent
/// for non-PDF sources, so neither is load-bearing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    /// Nearest preceding `## ` heading, without the marker.
    pub section: Option<String>,
    /// Nearest preceding page marker, 1-based.
    pub page: Option<usize>,
}

impl Anchor {
    pub fn is_empty(&self) -> bool {
        self.section.is_none() && self.page.is_none()
    }
}

/// Resolve the anchors covering `offset` (a byte index into `body`).
///
/// Offsets that fall before any marker resolve to an empty anchor rather than
/// guessing, and an offset past the end of the body is clamped.
pub fn resolve(body: &str, offset: usize) -> Anchor {
    let offset = offset.min(body.len());
    // Snap to a char boundary so slicing a multi-byte body cannot panic.
    let mut cut = offset;
    while cut > 0 && !body.is_char_boundary(cut) {
        cut -= 1;
    }
    let before = &body[..cut];

    Anchor {
        section: last_section(before),
        page: last_page(before),
    }
}

/// Nearest preceding `## ` heading. Headings are written on their own line by
/// the PDF ingest, so the match must start a line.
fn last_section(before: &str) -> Option<String> {
    // Nearest preceding heading, so the later one wins. Checking
    // `starts_with` first would pin every hit to the document's opening
    // heading no matter how far past it the match sits.
    let idx = match before.rfind("\n## ") {
        Some(i) => i + 1,
        None if before.starts_with("## ") => 0,
        None => return None,
    };
    let rest = &before[idx + 3..];
    let line = rest.split('\n').next().unwrap_or(rest).trim();
    if line.is_empty() {
        return None;
    }
    Some(line.to_string())
}

/// Nearest preceding page marker, parsed from `<!-- page N -->`.
///
/// Markers are written at the start of a line, and only those count: a paper
/// whose own text quotes the marker syntax (papers about markup do) would
/// otherwise report the quoted number as its page. `last_section` already
/// requires a line start; this is the same rule for the other marker.
fn last_page(before: &str) -> Option<usize> {
    let mut search_end = before.len();
    loop {
        let idx = before[..search_end].rfind(PAGE_MARKER_PREFIX)?;
        if idx == 0 || before.as_bytes()[idx - 1] == b'\n' {
            let rest = &before[idx + PAGE_MARKER_PREFIX.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            // A marker cut in half by the 500k body cap has no digits; keep
            // looking rather than reporting no page at all.
            if let Ok(page) = digits.parse() {
                return Some(page);
            }
        }
        search_end = idx;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "<!-- page 1 -->\nintro text\n## Methods\nwe did things\n<!-- page 2 -->\nmore method text\n## Results\nit worked\n";

    #[test]
    fn resolves_section_and_page() {
        let offset = BODY.find("we did things").unwrap();
        assert_eq!(
            resolve(BODY, offset),
            Anchor {
                section: Some("Methods".into()),
                page: Some(1)
            }
        );

        let offset = BODY.find("it worked").unwrap();
        assert_eq!(
            resolve(BODY, offset),
            Anchor {
                section: Some("Results".into()),
                page: Some(2)
            }
        );
    }

    /// A hit on page 2 that precedes any heading on that page still reports
    /// the last heading seen, which is the one it sits under.
    #[test]
    fn section_carries_across_a_page_break() {
        let offset = BODY.find("more method text").unwrap();
        assert_eq!(
            resolve(BODY, offset),
            Anchor {
                section: Some("Methods".into()),
                page: Some(2)
            }
        );
    }

    #[test]
    fn text_before_any_marker_has_no_anchor() {
        let body = "loose text with no markers at all";
        assert!(resolve(body, 5).is_empty());
    }

    /// Bodies stored before page markers existed still resolve their sections.
    #[test]
    fn legacy_body_resolves_section_without_page() {
        let body = "## Introduction\nold stored text";
        let offset = body.find("old stored").unwrap();
        assert_eq!(
            resolve(body, offset),
            Anchor {
                section: Some("Introduction".into()),
                page: None
            }
        );
    }

    #[test]
    fn offsets_are_clamped_and_snapped() {
        // Past the end: clamped, still resolves the last markers.
        let a = resolve(BODY, BODY.len() + 1000);
        assert_eq!(a.section.as_deref(), Some("Results"));
        assert_eq!(a.page, Some(2));

        // Mid-codepoint: snapped down instead of panicking.
        let body = "<!-- page 3 -->\n## Résumé\nnaïve café";
        let offset = body.find("café").unwrap() + 2;
        let anchor = resolve(body, offset);
        assert_eq!(anchor.page, Some(3));
        assert_eq!(anchor.section.as_deref(), Some("Résumé"));
    }

    /// A body that quotes the marker syntax must not be anchored by the quote.
    #[test]
    fn literal_marker_inside_text_is_not_a_page_boundary() {
        let body = "<!-- page 1 -->\n## Methods\nWe write markers like <!-- page 3 --> inline.\nreal text\n";
        let offset = body.find("real text").unwrap();
        assert_eq!(resolve(body, offset).page, Some(1));
    }

    /// The 500k body cap can slice a marker in half; a digitless remnant must
    /// fall through to the previous complete marker, not erase the page.
    #[test]
    fn truncated_trailing_marker_falls_through() {
        let body = "<!-- page 1 -->\ntext\n<!-- page ";
        assert_eq!(resolve(body, body.len()).page, Some(1));
    }

    /// The nearest preceding heading wins, not the document's first one.
    #[test]
    fn later_heading_beats_the_opening_heading() {
        let body = "## Intro\nalpha\n## Results\nbeta here\n";
        let offset = body.find("beta here").unwrap();
        assert_eq!(
            resolve(body, offset).section.as_deref(),
            Some("Results"),
            "a body that opens with a heading must not pin every hit to it"
        );
    }
}
