use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use utoipa::ToSchema;

use crate::libs::{db::AppState, error::ApiError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum TypeaheadScope {
    Companies,
    Tags,
    #[default]
    All,
}

#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct TypeaheadQuery {
    pub q: String,
    pub scope: Option<TypeaheadScope>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub struct TypeaheadItem {
    pub value: String,
    pub kind: String,
    pub slug: String,
}

use std::sync::{Arc, LazyLock, RwLock};

// In-memory cache entry with 60-second TTL
#[derive(Clone)]
struct CacheEntry {
    items: Vec<TypeaheadItem>,
    expires_at: Instant,
}

static TYPEAHEAD_CACHE: LazyLock<Arc<RwLock<HashMap<String, CacheEntry>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

/// Convert arbitrary text to URL-friendly lowercase slug
pub fn slugify(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut prev_dash = true; // prevent leading dash

    for ch in input.chars() {
        if ch.is_alphanumeric() {
            result.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            result.push('-');
            prev_dash = true;
        }
    }

    if result.ends_with('-') {
        result.pop();
    }

    result
}

/// Known standard Starknet / Cairo ecosystem companies and tags for high-speed fallback & seeding
pub const SEED_COMPANIES: &[&str] = &[
    "StarkWare",
    "Nethermind",
    "Kakarot zkEVM",
    "Argent",
    "Braavos",
    "Herodotus",
    "Pragma Oracle",
    "Ekubo Protocol",
    "AVNU",
    "Nostra Finance",
    "JediSwap",
    "ZkLend",
    "Carmine Finance",
    "Briq",
    "Dope Wars",
    "Loot Survivor",
    "Cartridge",
    "Madara",
    "Beerus",
    "Apibara",
];

pub const SEED_TAGS: &[&str] = &[
    "Cairo",
    "StarkNet",
    "DeFi",
    "zk-Rollup",
    "Account Abstraction",
    "Zero Knowledge",
    "Smart Contracts",
    "Layer 2",
    "Cairo 1.0",
    "Gaming",
    "AMM",
    "Lending",
    "Oracle",
    "Infrastructure",
    "Security",
    "Wallet",
    "Governance",
    "Tokenomics",
    "Full Node",
    "Sequencer",
];

/// Pure matching service logic: prefix and substring matching with score ranking
pub fn search_candidates(
    query: &str,
    scope: TypeaheadScope,
    limit: usize,
    extra_companies: &[String],
    extra_tags: &[String],
) -> Vec<TypeaheadItem> {
    let q_clean = query.trim().to_lowercase();
    if q_clean.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();

    // 1. Process Companies
    if scope == TypeaheadScope::All || scope == TypeaheadScope::Companies {
        let mut all_companies: Vec<String> = SEED_COMPANIES.iter().map(|s| s.to_string()).collect();
        for ec in extra_companies {
            if !all_companies.iter().any(|c| c.eq_ignore_ascii_case(ec)) {
                all_companies.push(ec.clone());
            }
        }

        for company in all_companies {
            let comp_lower = company.to_lowercase();
            if comp_lower.starts_with(&q_clean) || comp_lower.contains(&q_clean) {
                results.push((
                    TypeaheadItem {
                        slug: slugify(&company),
                        value: company.clone(),
                        kind: "company".to_string(),
                    },
                    comp_lower.starts_with(&q_clean), // prefix priority
                ));
            }
        }
    }

    // 2. Process Tags
    if scope == TypeaheadScope::All || scope == TypeaheadScope::Tags {
        let mut all_tags: Vec<String> = SEED_TAGS.iter().map(|s| s.to_string()).collect();
        for et in extra_tags {
            if !all_tags.iter().any(|t| t.eq_ignore_ascii_case(et)) {
                all_tags.push(et.clone());
            }
        }

        for tag in all_tags {
            let tag_lower = tag.to_lowercase();
            if tag_lower.starts_with(&q_clean) || tag_lower.contains(&q_clean) {
                results.push((
                    TypeaheadItem {
                        slug: slugify(&tag),
                        value: tag.clone(),
                        kind: "tag".to_string(),
                    },
                    tag_lower.starts_with(&q_clean), // prefix priority
                ));
            }
        }
    }

    // Sort: prefix matches first, then alphabetically
    results.sort_by(|(item_a, prefix_a), (item_b, prefix_b)| {
        match (prefix_b).cmp(prefix_a) {
            std::cmp::Ordering::Equal => item_a.value.cmp(&item_b.value),
            other => other,
        }
    });

    results.into_iter().take(limit).map(|(item, _)| item).collect()
}

#[utoipa::path(
    get,
    path = "/typeahead",
    tag = "search",
    params(TypeaheadQuery),
    responses(
        (status = 200, description = "Typeahead suggestions for companies and tags", body = Vec<TypeaheadItem>),
        (status = 400, description = "Bad request (query string missing or invalid)", body = crate::libs::error::ErrorBody),
        (status = 500, description = "Internal error", body = crate::libs::error::ErrorBody)
    )
)]
pub async fn typeahead(
    State(AppState { pool }): State<AppState>,
    Query(query): Query<TypeaheadQuery>,
) -> Result<Json<Vec<TypeaheadItem>>, ApiError> {
    let q_trimmed = query.q.trim();
    if q_trimmed.is_empty() {
        return Err(ApiError::BadRequest("Query string 'q' cannot be empty"));
    }

    let scope = query.scope.unwrap_or_default();
    let limit = query.limit.unwrap_or(10).clamp(1, 50) as usize;

    let cache_key = format!("{}:{}:{}", q_trimmed.to_lowercase(), match scope {
        TypeaheadScope::Companies => "companies",
        TypeaheadScope::Tags => "tags",
        TypeaheadScope::All => "all",
    }, limit);

    // 1. Check in-memory cache
    if let Ok(cache) = TYPEAHEAD_CACHE.read() {
        if let Some(entry) = cache.get(&cache_key) {
            if entry.expires_at > Instant::now() {
                return Ok(Json(entry.items.clone()));
            }
        }
    }

    // 2. Query distinct companies and tags from DB (reviews table) with fast fallback
    let extra_companies = match tokio::time::timeout(
        Duration::from_millis(150),
        sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT company FROM reviews WHERE company ILIKE $1 LIMIT 50",
        )
        .bind(format!("%{}%", q_trimmed))
        .fetch_all(&pool),
    )
    .await
    {
        Ok(Ok(list)) => list,
        _ => Vec::new(),
    };

    let extra_tags = match tokio::time::timeout(
        Duration::from_millis(150),
        sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT tag FROM reviews WHERE tag IS NOT NULL AND tag ILIKE $1 LIMIT 50",
        )
        .bind(format!("%{}%", q_trimmed))
        .fetch_all(&pool),
    )
    .await
    {
        Ok(Ok(list)) => list,
        _ => Vec::new(),
    };

    // 3. Compute matching candidates
    let items = search_candidates(q_trimmed, scope, limit, &extra_companies, &extra_tags);

    // 4. Update in-memory cache with 60s TTL
    if let Ok(mut cache) = TYPEAHEAD_CACHE.write() {
        cache.insert(
            cache_key,
            CacheEntry {
                items: items.clone(),
                expires_at: Instant::now() + Duration::from_secs(60),
            },
        );
    }

    Ok(Json(items))
}
