use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::Article;

#[derive(Clone, Copy, Debug, Default)]
struct Posting {
    title_hits: u16,
    body_hits: u16,
    metadata_hits: u16,
}

/// An in-memory lexical index over a set of validated corpus articles.
#[derive(Clone, Debug)]
pub struct CorpusIndex {
    articles: Vec<Article>,
    postings: BTreeMap<String, BTreeMap<usize, Posting>>,
}

/// A ranked article candidate with source provenance.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct QueryCandidate {
    /// Lexical score, weighted toward title matches.
    pub score: f64,
    /// Normalized query terms that matched this article.
    pub matched_terms: Vec<String>,
    /// SHA-256 hash of the source article.
    pub content_hash: String,
    /// Source title.
    pub title: String,
    /// Canonical source URL.
    pub url: String,
    /// Host derived from the URL.
    pub source_domain: String,
    /// Producer fetch timestamp.
    #[serde(with = "time::serde::rfc3339")]
    pub fetched_utc: OffsetDateTime,
    /// Age at the query observation point.
    pub age_seconds: i64,
}

/// The deterministic result of a lexical corpus query.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CorpusQueryResult {
    /// Query as supplied by the caller.
    pub query: String,
    /// Candidates, highest score first.
    pub candidates: Vec<QueryCandidate>,
    /// Query execution duration in milliseconds.
    pub latency_ms: u64,
    /// Query execution duration in nanoseconds.
    pub latency_ns: u64,
}

impl CorpusIndex {
    /// Builds a lexical inverted index from validated corpus articles.
    pub fn new(mut articles: Vec<Article>) -> Self {
        articles.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let mut postings = BTreeMap::<String, BTreeMap<usize, Posting>>::new();

        for (article_index, article) in articles.iter().enumerate() {
            add_tokens(
                &mut postings,
                article_index,
                tokenize(&article.title),
                |posting| {
                    posting.title_hits = posting.title_hits.saturating_add(1);
                },
            );
            add_tokens(
                &mut postings,
                article_index,
                tokenize(&article.body),
                |posting| {
                    posting.body_hits = posting.body_hits.saturating_add(1);
                },
            );
            add_tokens(
                &mut postings,
                article_index,
                tokenize(&format!("{} {}", article.url, article.source_domain)),
                |posting| {
                    posting.metadata_hits = posting.metadata_hits.saturating_add(1);
                },
            );
        }

        Self { articles, postings }
    }

    /// Returns the number of indexed articles.
    pub fn article_count(&self) -> usize {
        self.articles.len()
    }

    /// Returns an article by content hash when it remains indexed.
    pub fn article_by_content_hash(&self, content_hash: &str) -> Option<&Article> {
        self.articles
            .iter()
            .find(|article| article.content_hash == content_hash)
    }

    /// Runs a lexical lookup using the current UTC time to calculate candidate ages.
    pub fn query(&self, query: &str, limit: usize) -> CorpusQueryResult {
        self.query_at(query, limit, OffsetDateTime::now_utc())
    }

    /// Runs a lexical lookup at a supplied time, useful for deterministic callers and tests.
    pub fn query_at(&self, query: &str, limit: usize, now: OffsetDateTime) -> CorpusQueryResult {
        let started_at = Instant::now();
        let query_terms = tokenize(query).into_iter().collect::<Vec<_>>();
        // A common term can match almost the entire corpus. Keep the scoring representation
        // dense and defer both provenance cloning and ranking to the bounded result set; the
        // former implementation allocated one ordered map and one ordered term set per match,
        // then sorted every candidate even when the caller asked for only a handful.
        let mut scores = vec![0.0_f64; self.articles.len()];
        let mut matched_term_indexes = vec![Vec::<usize>::new(); self.articles.len()];
        let mut matched_articles = Vec::new();

        for (term_index, term) in query_terms.iter().enumerate() {
            let Some(postings) = self.postings.get(term) else {
                continue;
            };
            for (article_index, posting) in postings {
                let score = (f64::from(posting.title_hits) * 3.0)
                    + f64::from(posting.body_hits)
                    + f64::from(posting.metadata_hits);
                if scores[*article_index] == 0.0 {
                    matched_articles.push(*article_index);
                }
                scores[*article_index] += score;
                matched_term_indexes[*article_index].push(term_index);
            }
        }

        let candidate_order = |left: &usize, right: &usize| {
            let left_article = &self.articles[*left];
            let right_article = &self.articles[*right];
            scores[*right]
                .total_cmp(&scores[*left])
                .then_with(|| right_article.fetched_utc.cmp(&left_article.fetched_utc))
                .then_with(|| left_article.content_hash.cmp(&right_article.content_hash))
        };
        if matched_articles.len() > limit {
            matched_articles.select_nth_unstable_by(limit, candidate_order);
            matched_articles.truncate(limit);
        }
        matched_articles.sort_unstable_by(candidate_order);

        let candidates = matched_articles
            .into_iter()
            .map(|article_index| {
                let article = &self.articles[article_index];
                QueryCandidate {
                    score: scores[article_index],
                    matched_terms: matched_term_indexes[article_index]
                        .iter()
                        .map(|term_index| query_terms[*term_index].clone())
                        .collect(),
                    content_hash: article.content_hash.clone(),
                    title: article.title.clone(),
                    url: article.url.clone(),
                    source_domain: article.source_domain.clone(),
                    fetched_utc: article.fetched_utc,
                    age_seconds: article.age_seconds_at(now),
                }
            })
            .collect::<Vec<_>>();

        let elapsed = started_at.elapsed();
        CorpusQueryResult {
            query: query.to_string(),
            candidates,
            latency_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
            latency_ns: u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX),
        }
    }
}

/// Tokenizes text with the same simple lowercase-alphanumeric convention used by memory keyword
/// retrieval: terms shorter than three characters are omitted.
pub fn tokenize(input: &str) -> BTreeSet<String> {
    input
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(|term| {
            let normalized = term.trim().to_ascii_lowercase();
            (normalized.len() >= 3).then_some(normalized)
        })
        .collect()
}

fn add_tokens(
    postings: &mut BTreeMap<String, BTreeMap<usize, Posting>>,
    article_index: usize,
    tokens: BTreeSet<String>,
    update: impl Fn(&mut Posting),
) {
    for term in tokens {
        let posting = postings
            .entry(term)
            .or_default()
            .entry(article_index)
            .or_default();
        update(posting);
    }
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    use super::CorpusIndex;
    use crate::Article;

    fn article(id: usize, title: &str, body: &str) -> Article {
        Article {
            relative_path: format!("article-{id}.md"),
            content_hash: format!("hash-{id}"),
            url: format!("https://example.com/{id}"),
            title: title.to_string(),
            source_domain: "example.com".to_string(),
            fetched_utc: OffsetDateTime::parse("2026-07-08T00:00:00Z", &Rfc3339).unwrap(),
            body: body.to_string(),
        }
    }

    #[test]
    fn title_matches_rank_above_body_only_matches() {
        let index = CorpusIndex::new(vec![
            article(1, "A general update", "Artificial intelligence transition"),
            article(2, "Artificial intelligence transition", "A general update"),
        ]);

        let result = index.query_at(
            "artificial intelligence",
            4,
            OffsetDateTime::parse("2026-07-09T00:00:00Z", &Rfc3339).unwrap(),
        );

        assert_eq!(result.candidates[0].content_hash, "hash-2");
        assert_eq!(
            result.candidates[0].matched_terms,
            ["artificial", "intelligence"]
        );
        assert_eq!(result.candidates[0].age_seconds, 86_400);
    }

    #[test]
    fn lookup_stays_fast_on_a_synthetic_large_corpus() {
        let articles = (0..6_500)
            .map(|id| {
                let body = if id == 4_200 {
                    "A rare semantic transition signal appears here."
                } else {
                    "Routine corpus content about technology and systems."
                };
                article(id, &format!("Article {id}"), body)
            })
            .collect();
        let index = CorpusIndex::new(articles);

        let result = index.query("rare semantic transition", 3);

        assert_eq!(result.candidates[0].content_hash, "hash-4200");
        assert!(
            result.latency_ms < 300,
            "lookup took {} ms",
            result.latency_ms
        );
    }

    #[test]
    fn common_term_lookup_only_materializes_the_requested_top_candidates() {
        let index = CorpusIndex::new(
            (0..6_500)
                .map(|id| article(id, &format!("AI update {id}"), "AI transition news"))
                .collect(),
        );

        let result = index.query("ai transition", 3);

        assert_eq!(result.candidates.len(), 3);
        assert!(
            result.latency_ms < 300,
            "common-term lookup took {} ms",
            result.latency_ms
        );
    }
}
