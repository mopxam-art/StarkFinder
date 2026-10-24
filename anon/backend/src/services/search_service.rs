use std::collections::HashSet;
use std::sync::{LazyLock, RwLock};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use regex::Regex;

use crate::{
    libs::error::ErrorBody,
    models::search::{SearchItemType, SearchResponse, SearchResultItem},
};

pub const DEFAULT_LIMIT: usize = 10;
pub const MAX_LIMIT: usize = 25;
pub const SNIPPET_WINDOW: usize = 120;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum SearchError {
    #[error("Invalid query: {0}")]
    BadRequest(String),
}

impl IntoResponse for SearchError {
    fn into_response(self) -> Response {
        match self {
            SearchError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                Json(ErrorBody { error: msg }),
            )
                .into_response(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompanyDocument {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ReviewDocument {
    pub id: String,
    pub company_slug: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
}

/// In-memory search index supporting keyword scoring, trigrams, snippet highlighting, and pagination
pub struct SearchService {
    companies: RwLock<Vec<CompanyDocument>>,
    reviews: RwLock<Vec<ReviewDocument>>,
}

impl Default for SearchService {
    fn default() -> Self {
        Self::new_with_seed()
    }
}

pub static SEARCH_SERVICE: LazyLock<SearchService> = LazyLock::new(SearchService::default);

impl SearchService {
    pub fn new() -> Self {
        Self {
            companies: RwLock::new(Vec::new()),
            reviews: RwLock::new(Vec::new()),
        }
    }

    pub fn new_with_seed() -> Self {
        let service = Self::new();
        service.seed_default_data();
        service
    }

    pub fn clear(&self) {
        let mut comps = self.companies.write().unwrap();
        comps.clear();
        let mut revs = self.reviews.write().unwrap();
        revs.clear();
    }

    pub fn seed_default_data(&self) {
        self.clear();

        // Seed default companies
        self.index_company(
            "starkware",
            "StarkWare",
            "StarkWare develops STARK-based solutions for blockchain scalability, creating Starknet and Cairo programming language.",
            &["infrastructure", "zk-rollups", "cairo", "scalability"],
        );
        self.index_company(
            "argent",
            "Argent",
            "Argent is a leading smart contract wallet for Starknet and Ethereum, featuring social recovery and multisig security.",
            &["wallet", "account-abstraction", "defi", "mobile"],
        );
        self.index_company(
            "braavos",
            "Braavos",
            "Braavos is a self-custodial smart contract wallet on Starknet with hardware signer protection and biometric authentication.",
            &["wallet", "security", "hardware-signer", "account-abstraction"],
        );
        self.index_company(
            "avnu",
            "AVNU",
            "AVNU is the premier decentralized liquidity aggregator on Starknet, providing optimal execution and zero slippage routes.",
            &["dex", "aggregator", "defi", "liquidity"],
        );
        self.index_company(
            "ekubo",
            "Ekubo Protocol",
            "Ekubo is a concentrated liquidity automated market maker built specifically for Starknet with extreme gas efficiency.",
            &["amm", "dex", "defi", "concentrated-liquidity"],
        );
        self.index_company(
            "cartridge",
            "Cartridge",
            "Cartridge builds infrastructure and controller tools for autonomous worlds and on-chain gaming on Starknet with Dojo engine.",
            &["gaming", "infrastructure", "dojo", "controller"],
        );

        // Seed default reviews
        self.index_review(
            "rev-001",
            "starkware",
            "Leading cutting edge zero knowledge cryptography",
            "Working at StarkWare has been an incredible journey. The team is pushing the boundaries of math and zero knowledge proofs with Cairo.",
            &["culture", "math", "zk"],
        );
        self.index_review(
            "rev-002",
            "argent",
            "Fast paced fintech and account abstraction innovations",
            "Argent provides fantastic engineering challenges in mobile development and smart contract wallet recovery systems. Great work life balance.",
            &["mobile", "engineering", "wallet"],
        );
        self.index_review(
            "rev-003",
            "braavos",
            "High security standards and developer autonomy",
            "Braavos has strong cryptographic leadership. The hardware signer implementation sets the standard for modern Web3 UX.",
            &["security", "cryptography"],
        );
        self.index_review(
            "rev-004",
            "avnu",
            "Great DeFi engineering team and low latency DEX aggregation",
            "At AVNU we optimize routing algorithms and arbitrage pipelines across Starknet liquidity pools. Fast execution and great rewards.",
            &["defi", "algorithms"],
        );
        self.index_review(
            "rev-005",
            "cartridge",
            "Pioneering on-chain gaming and Dojo contracts",
            "Cartridge is defining how fully on-chain gaming works. Cairo 1.0 contracts and controller tooling make development exciting.",
            &["gaming", "cairo"],
        );
    }

    pub fn index_company(&self, slug: &str, name: &str, description: &str, tags: &[&str]) {
        let mut comps = self.companies.write().unwrap();
        // Remove previous if exists
        comps.retain(|c| c.slug != slug);
        comps.push(CompanyDocument {
            slug: slug.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
        });
    }

    pub fn index_review(
        &self,
        id: &str,
        company_slug: &str,
        title: &str,
        content: &str,
        tags: &[&str],
    ) {
        let mut revs = self.reviews.write().unwrap();
        revs.retain(|r| r.id != id);
        revs.push(ReviewDocument {
            id: id.to_string(),
            company_slug: company_slug.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
        });
    }

    /// Execute search across indexed documents
    pub fn search(
        &self,
        raw_query: &str,
        raw_scope: Option<&str>,
        raw_limit: Option<usize>,
        raw_offset: Option<usize>,
    ) -> Result<SearchResponse, SearchError> {
        let trimmed_query = raw_query.trim();
        if trimmed_query.len() < 2 {
            return Err(SearchError::BadRequest(
                "query must be at least 2 characters".to_string(),
            ));
        }

        let scope_str = raw_scope.unwrap_or("all").trim().to_lowercase();
        let scope = match scope_str.as_str() {
            "reviews" => "reviews",
            "companies" => "companies",
            _ => "all",
        };

        let limit = match raw_limit {
            None => DEFAULT_LIMIT,
            Some(0) => DEFAULT_LIMIT,
            Some(l) => l.min(MAX_LIMIT),
        };
        let offset = raw_offset.unwrap_or(0);

        let mut matches = Vec::new();

        // 1. Search companies if scope is "all" or "companies"
        if scope == "all" || scope == "companies" {
            let comps = self.companies.read().unwrap();
            for comp in comps.iter() {
                if let Some(score) = score_company(comp, trimmed_query) {
                    let snippet = generate_snippet(&comp.description, trimmed_query);
                    matches.push(SearchResultItem {
                        item_type: SearchItemType::Company,
                        id_or_slug: comp.slug.clone(),
                        title: Some(comp.name.clone()),
                        score,
                        snippet,
                    });
                }
            }
        }

        // 2. Search reviews if scope is "all" or "reviews"
        if scope == "all" || scope == "reviews" {
            let revs = self.reviews.read().unwrap();
            for rev in revs.iter() {
                if let Some(score) = score_review(rev, trimmed_query) {
                    let snippet = generate_snippet(&rev.content, trimmed_query);
                    matches.push(SearchResultItem {
                        item_type: SearchItemType::Review,
                        id_or_slug: rev.id.clone(),
                        title: Some(rev.title.clone()),
                        score,
                        snippet,
                    });
                }
            }
        }

        // 3. Sort by score descending, then by id_or_slug ascending (deterministic tie-breaking)
        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id_or_slug.cmp(&b.id_or_slug))
        });

        let total = matches.len();
        let (paginated_items, has_more) = if offset >= total {
            (Vec::new(), false)
        } else {
            let end = (offset + limit).min(total);
            (matches[offset..end].to_vec(), end < total)
        };

        Ok(SearchResponse {
            query: trimmed_query.to_string(),
            scope: scope.to_string(),
            total,
            items: paginated_items,
            has_more,
        })
    }
}

/// Calculate relevance score for a company document
fn score_company(comp: &CompanyDocument, query: &str) -> Option<f64> {
    let q_lower = query.to_lowercase();
    let name_lower = comp.name.to_lowercase();
    let slug_lower = comp.slug.to_lowercase();
    let desc_lower = comp.description.to_lowercase();

    let mut score = 0.0;

    // Exact matches
    if name_lower == q_lower || slug_lower == q_lower {
        score += 10.0;
    } else if name_lower.starts_with(&q_lower) || slug_lower.starts_with(&q_lower) {
        score += 6.0;
    } else if name_lower.contains(&q_lower) || slug_lower.contains(&q_lower) {
        score += 4.0;
    }

    // Tag matches
    for tag in &comp.tags {
        let t_lower = tag.to_lowercase();
        if t_lower == q_lower {
            score += 3.0;
        } else if t_lower.contains(&q_lower) {
            score += 1.5;
        }
    }

    // Description match
    if desc_lower.contains(&q_lower) {
        score += 2.0;
    }

    // Multi-word token matching
    let tokens: Vec<&str> = q_lower.split_whitespace().collect();
    if tokens.len() > 1 {
        let mut token_matches = 0;
        for token in &tokens {
            if name_lower.contains(token) || slug_lower.contains(token) || desc_lower.contains(token) {
                token_matches += 1;
            }
        }
        if token_matches == tokens.len() {
            score += 3.0;
        } else if token_matches > 0 {
            score += 0.5 * token_matches as f64;
        }
    }

    // Trigram similarity boost
    let sim = trigram_similarity(&q_lower, &name_lower);
    if sim > 0.3 {
        score += sim * 2.0;
    }

    if score > 0.0 {
        Some((score * 100.0).round() / 100.0)
    } else {
        None
    }
}

/// Calculate relevance score for a review document
fn score_review(rev: &ReviewDocument, query: &str) -> Option<f64> {
    let q_lower = query.to_lowercase();
    let title_lower = rev.title.to_lowercase();
    let slug_lower = rev.company_slug.to_lowercase();
    let content_lower = rev.content.to_lowercase();

    let mut score = 0.0;

    // Exact matches
    if title_lower == q_lower {
        score += 8.0;
    } else if title_lower.starts_with(&q_lower) {
        score += 5.0;
    } else if title_lower.contains(&q_lower) {
        score += 3.5;
    }

    if slug_lower == q_lower {
        score += 3.0;
    } else if slug_lower.contains(&q_lower) {
        score += 1.5;
    }

    // Content match
    if content_lower.contains(&q_lower) {
        score += 2.0;
    }

    // Tag matches
    for tag in &rev.tags {
        let t_lower = tag.to_lowercase();
        if t_lower == q_lower {
            score += 2.5;
        } else if t_lower.contains(&q_lower) {
            score += 1.0;
        }
    }

    // Multi-word token matching
    let tokens: Vec<&str> = q_lower.split_whitespace().collect();
    if tokens.len() > 1 {
        let mut token_matches = 0;
        for token in &tokens {
            if title_lower.contains(token) || content_lower.contains(token) || slug_lower.contains(token) {
                token_matches += 1;
            }
        }
        if token_matches == tokens.len() {
            score += 2.5;
        } else if token_matches > 0 {
            score += 0.4 * token_matches as f64;
        }
    }

    // Trigram similarity boost
    let sim = trigram_similarity(&q_lower, &title_lower);
    if sim > 0.3 {
        score += sim * 2.0;
    }

    if score > 0.0 {
        Some((score * 100.0).round() / 100.0)
    } else {
        None
    }
}

/// Compute trigram character similarity (Jaccard similarity of 3-grams)
fn trigram_similarity(s1: &str, s2: &str) -> f64 {
    let t1 = get_trigrams(s1);
    let t2 = get_trigrams(s2);

    if t1.is_empty() || t2.is_empty() {
        return 0.0;
    }

    let intersection = t1.intersection(&t2).count();
    let union = t1.union(&t2).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

fn get_trigrams(s: &str) -> HashSet<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut trigrams = HashSet::new();
    if chars.len() < 3 {
        return trigrams;
    }
    for i in 0..=(chars.len() - 3) {
        trigrams.insert(chars[i..i + 3].iter().collect());
    }
    trigrams
}

/// Generate snippet with <mark> highlighting and surrounding ellipsis within ~120 chars window
pub fn generate_snippet(text: &str, query: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let q_lower = query.to_lowercase();
    let text_lower = trimmed.to_lowercase();

    // Determine the best match position
    let match_pos = text_lower.find(&q_lower).or_else(|| {
        // Try individual words if query is multi-word
        query
            .split_whitespace()
            .filter(|w| w.len() >= 2)
            .find_map(|w| text_lower.find(&w.to_lowercase()))
    });

    let (start_idx, end_idx) = match match_pos {
        Some(pos) => {
            let half = SNIPPET_WINDOW / 2;
            let start = pos.saturating_sub(half);
            let end = (pos + query.len() + half).min(trimmed.len());

            // Adjust to align to character boundaries
            let mut s = start;
            while s > 0 && !trimmed.is_char_boundary(s) {
                s -= 1;
            }
            let mut e = end;
            while e < trimmed.len() && !trimmed.is_char_boundary(e) {
                e += 1;
            }
            (s, e)
        }
        None => {
            let mut e = SNIPPET_WINDOW.min(trimmed.len());
            while e < trimmed.len() && !trimmed.is_char_boundary(e) {
                e += 1;
            }
            (0, e)
        }
    };

    let slice = &trimmed[start_idx..end_idx];

    // Highlight query keywords using regex with case-insensitivity
    let highlighted = highlight_terms(slice, query);

    let mut result = String::new();
    if start_idx > 0 {
        result.push_str("... ");
    }
    result.push_str(&highlighted);
    if end_idx < trimmed.len() {
        result.push_str(" ...");
    }

    result
}

/// Highlight matching terms with <mark>...</mark>
fn highlight_terms(text: &str, query: &str) -> String {
    let tokens: Vec<&str> = query
        .split_whitespace()
        .filter(|t| t.len() >= 2)
        .collect();

    if tokens.is_empty() {
        return text.to_string();
    }

    let mut pattern = String::from("(?i)(");
    for (i, token) in tokens.iter().enumerate() {
        if i > 0 {
            pattern.push('|');
        }
        pattern.push_str(&regex::escape(token));
    }
    pattern.push(')');

    if let Ok(re) = Regex::new(&pattern) {
        re.replace_all(text, "<mark>$1</mark>").to_string()
    } else {
        text.to_string()
    }
}
