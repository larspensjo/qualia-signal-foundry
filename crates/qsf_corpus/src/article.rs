use sha2::{Digest, Sha256};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use url::Url;

/// A validated external article, retained with enough data to rebuild the lexical index.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Article {
    /// Corpus-relative source path, normalized with `/` separators.
    pub relative_path: String,
    /// SHA-256 hash of the complete source file.
    pub content_hash: String,
    /// Producer-provided canonical source URL.
    pub url: String,
    /// Article title from required frontmatter.
    pub title: String,
    /// Host derived from the source URL.
    pub source_domain: String,
    /// Producer fetch timestamp.
    #[serde(with = "time::serde::rfc3339")]
    pub fetched_utc: OffsetDateTime,
    /// Article body after the closing frontmatter delimiter.
    pub body: String,
}

impl Article {
    /// Computes non-negative article age in seconds at a supplied observation time.
    pub fn age_seconds_at(&self, now: OffsetDateTime) -> i64 {
        (now - self.fetched_utc).whole_seconds().max(0)
    }
}

/// Why an article could not safely enter the corpus.
#[derive(Debug, Error)]
pub enum ArticleParseError {
    /// The document does not begin with a complete `---` frontmatter block.
    #[error("article has no complete frontmatter block")]
    MissingFrontmatter,
    /// A required frontmatter field is absent or blank.
    #[error("article frontmatter is missing required field `{field}`")]
    MissingRequiredField {
        /// Missing field name.
        field: &'static str,
    },
    /// A required frontmatter field appeared more than once.
    #[error("article frontmatter contains duplicate required field `{field}`")]
    DuplicateRequiredField {
        /// Duplicate field name.
        field: &'static str,
    },
    /// The required URL could not provide a source host.
    #[error("article frontmatter URL is invalid: {value}")]
    InvalidUrl {
        /// Invalid supplied URL.
        value: String,
    },
    /// The fetch time was not RFC 3339 UTC-compatible data.
    #[error("article frontmatter fetched_utc is invalid: {value}")]
    InvalidFetchedUtc {
        /// Invalid supplied timestamp.
        value: String,
    },
}

/// Parses one UTF-8 markdown article with required source frontmatter.
pub fn parse_article(relative_path: String, source: &str) -> Result<Article, ArticleParseError> {
    let mut lines = source.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err(ArticleParseError::MissingFrontmatter);
    }

    let mut url = None;
    let mut title = None;
    let mut fetched_utc = None;
    let mut body_start_line = None;

    for (index, line) in source.lines().enumerate().skip(1) {
        if line.trim() == "---" {
            body_start_line = Some(index + 1);
            break;
        }
        let Some((key, raw_value)) = line.split_once(':') else {
            continue;
        };
        let value = strip_optional_quotes(raw_value.trim()).to_string();
        match key.trim() {
            "url" => assign_required(&mut url, value, "url")?,
            "title" => assign_required(&mut title, value, "title")?,
            "fetched_utc" => assign_required(&mut fetched_utc, value, "fetched_utc")?,
            _ => {}
        }
    }

    let body_start_line = body_start_line.ok_or(ArticleParseError::MissingFrontmatter)?;
    let url = required_value(url, "url")?;
    let title = required_value(title, "title")?;
    let fetched_utc = required_value(fetched_utc, "fetched_utc")?;
    let parsed_url =
        Url::parse(&url).map_err(|_| ArticleParseError::InvalidUrl { value: url.clone() })?;
    let source_domain = parsed_url
        .host_str()
        .map(str::to_string)
        .ok_or_else(|| ArticleParseError::InvalidUrl { value: url.clone() })?;
    let fetched_utc = OffsetDateTime::parse(&fetched_utc, &Rfc3339).map_err(|_| {
        ArticleParseError::InvalidFetchedUtc {
            value: fetched_utc.clone(),
        }
    })?;
    let body = source
        .lines()
        .skip(body_start_line)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Article {
        relative_path,
        content_hash: content_hash(source),
        url,
        title,
        source_domain,
        fetched_utc,
        body,
    })
}

/// Calculates the SHA-256 content hash used by the incremental ledger.
pub fn content_hash(source: &str) -> String {
    Sha256::digest(source.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn assign_required(
    target: &mut Option<String>,
    value: String,
    field: &'static str,
) -> Result<(), ArticleParseError> {
    if target.replace(value).is_some() {
        return Err(ArticleParseError::DuplicateRequiredField { field });
    }
    Ok(())
}

fn required_value(value: Option<String>, field: &'static str) -> Result<String, ArticleParseError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or(ArticleParseError::MissingRequiredField { field })
}

fn strip_optional_quotes(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::{ArticleParseError, parse_article};

    #[test]
    fn parses_required_fields_and_ignores_unknown_frontmatter() {
        let article = parse_article(
            "linked/example.md".to_string(),
            "---\ntitle: Example: a colon\nurl: https://example.com/path\nfetched_utc: 2026-07-09T10:00:00Z\nunknown: ignored\n---\nBody text.",
        )
        .unwrap();

        assert_eq!(article.title, "Example: a colon");
        assert_eq!(article.source_domain, "example.com");
        assert_eq!(article.body, "Body text.");
    }

    #[test]
    fn rejects_malformed_required_frontmatter() {
        let error =
            parse_article("bad.md".to_string(), "---\ntitle: Missing URL\n---\nbody").unwrap_err();

        assert!(matches!(
            error,
            ArticleParseError::MissingRequiredField { field: "url" }
        ));
    }
}
