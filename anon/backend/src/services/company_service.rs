use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, RwLock};

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    libs::error::ErrorBody,
    models::company::{
        CompanyListResponse, CompanyResponse, CreateCompanyRequest, UpdateCompanyRequest,
    },
};

pub const MIN_NAME_LEN: usize = 2;
pub const MAX_NAME_LEN: usize = 80;
pub const MAX_DESCRIPTION_LEN: usize = 1000;
pub const MAX_DOMAIN_LEN: usize = 253;
pub const MAX_LABEL_LEN: usize = 63;

pub static COMPANY_SERVICE: LazyLock<CompanyService> = LazyLock::new(CompanyService::new);

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum CompanyError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Forbidden: {0}")]
    Forbidden(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for CompanyError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            CompanyError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            CompanyError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            CompanyError::Forbidden(m) => (StatusCode::FORBIDDEN, m.clone()),
            CompanyError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            CompanyError::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
            CompanyError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
        };
        (status, Json(ErrorBody { error: msg })).into_response()
    }
}

pub fn canonical_wallet(wallet: &str) -> String {
    let trimmed = wallet.trim().to_lowercase();
    crate::libs::wallet::normalize_and_validate(&trimmed).unwrap_or(trimmed)
}

#[derive(Debug, Clone)]
pub struct CompanyRecord {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub domain: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<CompanyRecord> for CompanyResponse {
    fn from(r: CompanyRecord) -> Self {
        CompanyResponse {
            id: r.id,
            name: r.name,
            slug: r.slug,
            domain: r.domain,
            description: r.description,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

pub struct CompanyService {
    companies: RwLock<HashMap<Uuid, CompanyRecord>>,
    slug_to_id: RwLock<HashMap<String, Uuid>>,
    domain_to_id: RwLock<HashMap<String, Uuid>>,
    admins: RwLock<HashSet<String>>,
}

impl Default for CompanyService {
    fn default() -> Self {
        Self::new()
    }
}

impl CompanyService {
    pub fn new() -> Self {
        let default_admins = ["0x01", "0x0123", "0xadmin1234567890"];
        let mut admin_set = HashSet::new();
        for a in default_admins {
            admin_set.insert(canonical_wallet(a));
        }

        Self {
            companies: RwLock::new(HashMap::new()),
            slug_to_id: RwLock::new(HashMap::new()),
            domain_to_id: RwLock::new(HashMap::new()),
            admins: RwLock::new(admin_set),
        }
    }

    pub fn reset(&self) {
        let mut c = self.companies.write().unwrap();
        let mut s = self.slug_to_id.write().unwrap();
        let mut d = self.domain_to_id.write().unwrap();
        c.clear();
        s.clear();
        d.clear();
    }

    pub fn register_admin(&self, wallet: &str) {
        let norm = canonical_wallet(wallet);
        let mut ad = self.admins.write().unwrap();
        ad.insert(norm);
    }

    pub fn is_admin(&self, wallet: &str) -> bool {
        let norm = canonical_wallet(wallet);
        let ad = self.admins.read().unwrap();
        ad.contains(&norm)
    }

    pub fn validate_name(name: &str) -> Result<String, CompanyError> {
        let trimmed = name.trim();
        let len = trimmed.chars().count();
        if !(MIN_NAME_LEN..=MAX_NAME_LEN).contains(&len) {
            return Err(CompanyError::BadRequest(format!(
                "name must be between {} and {} characters",
                MIN_NAME_LEN, MAX_NAME_LEN
            )));
        }
        Ok(trimmed.to_string())
    }

    pub fn validate_description(description: Option<&str>) -> Result<Option<String>, CompanyError> {
        match description {
            Some(d) => {
                let trimmed = d.trim();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    if trimmed.chars().count() > MAX_DESCRIPTION_LEN {
                        return Err(CompanyError::BadRequest(format!(
                            "description must not exceed {} characters",
                            MAX_DESCRIPTION_LEN
                        )));
                    }
                    Ok(Some(trimmed.to_string()))
                }
            }
            None => Ok(None),
        }
    }

    pub fn validate_domain(domain: Option<&str>) -> Result<Option<String>, CompanyError> {
        match domain {
            Some(d) => {
                let trimmed = d.trim().to_lowercase();
                if trimmed.is_empty() {
                    return Ok(None);
                }

                if trimmed.starts_with("http://")
                    || trimmed.starts_with("https://")
                    || trimmed.contains('/')
                    || trimmed.contains(':')
                    || trimmed.contains('@')
                    || trimmed.contains(' ')
                {
                    return Err(CompanyError::BadRequest(
                        "invalid RFC domain format: protocols, paths and ports are not allowed"
                            .to_string(),
                    ));
                }

                if trimmed.len() > MAX_DOMAIN_LEN || !trimmed.contains('.') {
                    return Err(CompanyError::BadRequest(
                        "invalid RFC domain format: must contain at least one dot and be <= 253 characters"
                            .to_string(),
                    ));
                }

                let labels: Vec<&str> = trimmed.split('.').collect();
                if labels.len() < 2 {
                    return Err(CompanyError::BadRequest(
                        "invalid RFC domain format: minimum two labels required".to_string(),
                    ));
                }

                for label in &labels {
                    if label.is_empty()
                        || label.len() > MAX_LABEL_LEN
                        || label.starts_with('-')
                        || label.ends_with('-')
                    {
                        return Err(CompanyError::BadRequest(format!(
                            "invalid RFC domain label '{}': must be 1-63 chars and not start/end with hyphen",
                            label
                        )));
                    }

                    if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                        return Err(CompanyError::BadRequest(format!(
                            "invalid RFC domain label '{}': must only contain alphanumeric characters or hyphens",
                            label
                        )));
                    }
                }

                // Check TLD is at least 2 chars and purely alphabetic
                let tld = labels.last().unwrap();
                if tld.len() < 2 || !tld.chars().all(|c| c.is_ascii_alphabetic()) {
                    return Err(CompanyError::BadRequest(
                        "invalid RFC domain format: top-level domain must be at least 2 alphabetic characters"
                            .to_string(),
                    ));
                }

                Ok(Some(trimmed))
            }
            None => Ok(None),
        }
    }

    pub fn slugify(name: &str) -> String {
        let mut slug = String::new();
        let mut last_was_dash = false;

        for ch in name.trim().chars() {
            if ch.is_ascii_alphanumeric() {
                slug.push(ch.to_ascii_lowercase());
                last_was_dash = false;
            } else if !last_was_dash {
                slug.push('-');
                last_was_dash = true;
            }
        }

        let trimmed = slug.trim_matches('-');
        if trimmed.is_empty() {
            "company".to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn generate_unique_slug(&self, name: &str) -> String {
        let base_slug = Self::slugify(name);
        let s_map = self.slug_to_id.read().unwrap();

        if !s_map.contains_key(&base_slug) {
            return base_slug;
        }

        let mut counter = 2;
        loop {
            let candidate = format!("{}-{}", base_slug, counter);
            if !s_map.contains_key(&candidate) {
                return candidate;
            }
            counter += 1;
        }
    }

    pub fn create_company(&self, req: CreateCompanyRequest) -> Result<CompanyResponse, CompanyError> {
        let validated_name = Self::validate_name(&req.name)?;
        let validated_desc = Self::validate_description(req.description.as_deref())?;
        let validated_domain = Self::validate_domain(req.domain.as_deref())?;

        let mut c_map = self.companies.write().unwrap();
        let mut s_map = self.slug_to_id.write().unwrap();
        let mut d_map = self.domain_to_id.write().unwrap();

        // Check domain uniqueness if provided
        if let Some(ref d) = validated_domain {
            if d_map.contains_key(d) {
                return Err(CompanyError::Conflict(format!(
                    "domain '{}' is already registered to another company",
                    d
                )));
            }
        }

        // Generate unique slug
        let base_slug = Self::slugify(&validated_name);
        let unique_slug = if !s_map.contains_key(&base_slug) {
            base_slug
        } else {
            let mut counter = 2;
            loop {
                let candidate = format!("{}-{}", base_slug, counter);
                if !s_map.contains_key(&candidate) {
                    break candidate;
                }
                counter += 1;
            }
        };

        let now = Utc::now();
        let id = Uuid::new_v4();

        let record = CompanyRecord {
            id,
            name: validated_name,
            slug: unique_slug.clone(),
            domain: validated_domain.clone(),
            description: validated_desc,
            created_at: now,
            updated_at: now,
        };

        s_map.insert(unique_slug, id);
        if let Some(ref d) = validated_domain {
            d_map.insert(d.clone(), id);
        }
        c_map.insert(id, record.clone());

        Ok(record.into())
    }

    pub fn get_company_by_slug(&self, slug: &str) -> Result<CompanyResponse, CompanyError> {
        let norm_slug = slug.trim().to_lowercase();
        let s_map = self.slug_to_id.read().unwrap();
        let id = s_map
            .get(&norm_slug)
            .ok_or_else(|| CompanyError::NotFound(format!("company with slug '{}' not found", slug)))?;

        let c_map = self.companies.read().unwrap();
        let record = c_map
            .get(id)
            .ok_or_else(|| CompanyError::NotFound(format!("company with slug '{}' not found", slug)))?;

        Ok(record.clone().into())
    }

    pub fn update_company(
        &self,
        admin_wallet: &str,
        slug: &str,
        req: UpdateCompanyRequest,
    ) -> Result<CompanyResponse, CompanyError> {
        if !self.is_admin(admin_wallet) {
            return Err(CompanyError::Forbidden(
                "action requires administrator privileges".to_string(),
            ));
        }

        let norm_slug = slug.trim().to_lowercase();
        let s_map = self.slug_to_id.read().unwrap();
        let id = *s_map
            .get(&norm_slug)
            .ok_or_else(|| CompanyError::NotFound(format!("company with slug '{}' not found", slug)))?;

        let mut c_map = self.companies.write().unwrap();
        let mut d_map = self.domain_to_id.write().unwrap();

        let record = c_map.get_mut(&id).unwrap();

        // Validate new name if provided
        if let Some(ref name) = req.name {
            let validated_name = Self::validate_name(name)?;
            record.name = validated_name;
        }

        // Validate new description if provided
        if let Some(ref desc) = req.description {
            let validated_desc = Self::validate_description(Some(desc.as_str()))?;
            record.description = validated_desc;
        }

        // Validate new domain if provided
        if let Some(ref domain) = req.domain {
            let validated_domain = Self::validate_domain(Some(domain.as_str()))?;
            if let Some(ref new_d) = validated_domain {
                if let Some(&existing_id) = d_map.get(new_d) {
                    if existing_id != id {
                        return Err(CompanyError::Conflict(format!(
                            "domain '{}' is already registered to another company",
                            new_d
                        )));
                    }
                }
            }

            // Remove old domain from map if present
            if let Some(ref old_d) = record.domain {
                d_map.remove(old_d);
            }

            // Insert new domain if present
            if let Some(ref new_d) = validated_domain {
                d_map.insert(new_d.clone(), id);
            }

            record.domain = validated_domain;
        }

        record.updated_at = Utc::now();
        Ok(record.clone().into())
    }

    pub fn list_companies(&self, limit: usize, offset: usize) -> CompanyListResponse {
        let capped_limit = limit.clamp(1, 100);
        let c_map = self.companies.read().unwrap();

        let mut all: Vec<CompanyResponse> = c_map.values().cloned().map(Into::into).collect();
        all.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));

        let total = all.len();
        let items: Vec<CompanyResponse> = all.into_iter().skip(offset).take(capped_limit).collect();

        CompanyListResponse {
            items,
            total,
            limit: capped_limit,
            offset,
        }
    }
}
