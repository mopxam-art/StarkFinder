use chrono::Utc;
use std::str::FromStr;
use std::sync::{LazyLock, RwLock};
use uuid::Uuid;

use crate::libs::error::ApiError;
use crate::models::salary::{
    CompensationType, CurrencyCode, PercentileStats, SalaryAggregate, SalaryReport,
    SubmitSalaryReq,
};

pub const PRIVACY_THRESHOLD: usize = 3;

pub fn sanitize_text(input: &str) -> Result<String, ApiError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest("field cannot be empty"));
    }

    let upper = trimmed.to_uppercase();
    if upper.contains("DROP TABLE")
        || upper.contains("UNION SELECT")
        || upper.contains("SELECT *")
        || upper.contains("--")
        || upper.contains("/*")
        || upper.contains(";")
        || upper.contains("<SCRIPT")
    {
        return Err(ApiError::BadRequest("invalid characters detected in field"));
    }

    Ok(trimmed.to_string())
}

pub fn percentile_linear(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return 0.0;
    }
    if n == 1 {
        return sorted[0];
    }

    let idx = p * (n - 1) as f64;
    let lower = idx.floor() as usize;
    let upper = idx.ceil() as usize;

    if lower == upper {
        sorted[lower]
    } else {
        let frac = idx - lower as f64;
        sorted[lower] + frac * (sorted[upper] - sorted[lower])
    }
}

pub fn compute_percentiles(values: &[f64]) -> Option<PercentileStats> {
    if values.is_empty() {
        return None;
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = sorted.len();
    let min = sorted[0];
    let max = sorted[n - 1];
    let sum: f64 = sorted.iter().sum();
    let average = (sum / n as f64 * 100.0).round() / 100.0;

    let p25 = (percentile_linear(&sorted, 0.25) * 100.0).round() / 100.0;
    let median = (percentile_linear(&sorted, 0.50) * 100.0).round() / 100.0;
    let p75 = (percentile_linear(&sorted, 0.75) * 100.0).round() / 100.0;
    let p90 = (percentile_linear(&sorted, 0.90) * 100.0).round() / 100.0;

    Some(PercentileStats {
        min: (min * 100.0).round() / 100.0,
        p25,
        median,
        p75,
        p90,
        max: (max * 100.0).round() / 100.0,
        average,
    })
}

pub struct SalaryStore {
    reports: RwLock<Vec<SalaryReport>>,
    privacy_threshold: usize,
}

impl Default for SalaryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SalaryStore {
    pub fn new() -> Self {
        Self {
            reports: RwLock::new(Vec::new()),
            privacy_threshold: PRIVACY_THRESHOLD,
        }
    }

    pub fn with_threshold(threshold: usize) -> Self {
        Self {
            reports: RwLock::new(Vec::new()),
            privacy_threshold: threshold,
        }
    }

    pub fn clear(&self) {
        self.reports.write().unwrap().clear();
    }

    pub fn len(&self) -> usize {
        self.reports.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn submit_report(
        &self,
        req: SubmitSalaryReq,
        user_wallet: Option<String>,
    ) -> Result<SalaryReport, ApiError> {
        let company_id = sanitize_text(&req.company_id)?;
        let job_title = sanitize_text(&req.job_title)?;

        // Validate base salary
        if !req.base_salary.is_finite() || req.base_salary <= 0.0 {
            return Err(ApiError::BadRequest("base salary must be greater than zero"));
        }
        if req.base_salary > 100_000_000.0 {
            return Err(ApiError::BadRequest("base salary exceeds maximum allowed limit"));
        }

        // Validate bonus
        let bonus = match req.bonus {
            Some(b) => {
                if !b.is_finite() || b < 0.0 {
                    return Err(ApiError::BadRequest("bonus cannot be negative"));
                }
                b
            }
            None => 0.0,
        };

        // Validate equity / tokens
        let equity_or_tokens = match req.equity_or_tokens {
            Some(eq) => {
                if !eq.is_finite() || eq < 0.0 {
                    return Err(ApiError::BadRequest("equity or tokens cannot be negative"));
                }
                eq
            }
            None => 0.0,
        };

        // Validate currency code
        let currency = CurrencyCode::from_str(&req.currency).map_err(|_| {
            ApiError::BadRequest("unsupported currency: must be USD, EUR, GBP, USDC, ETH, or STRK")
        })?;

        // Parse compensation type
        let compensation_type = match req.compensation_type {
            Some(ref ct) => CompensationType::from_str(ct).map_err(|_| {
                ApiError::BadRequest("invalid compensation type: must be full_time, contract, part_time, or hourly")
            })?,
            None => CompensationType::FullTime,
        };

        let total_compensation = req.base_salary + bonus + equity_or_tokens;

        let report = SalaryReport {
            id: Uuid::new_v4().to_string(),
            company_id: company_id.to_lowercase(),
            job_title,
            base_salary: req.base_salary,
            currency,
            compensation_type,
            bonus,
            equity_or_tokens,
            total_compensation,
            years_of_experience: req.years_of_experience,
            location: req.location.map(|l| l.trim().to_string()),
            user_wallet,
            created_at: Utc::now(),
        };

        let mut reports = self.reports.write().unwrap();
        reports.push(report.clone());

        Ok(report)
    }

    pub fn get_company_summary(
        &self,
        company_id: &str,
        role_filter: Option<&str>,
        target_currency: Option<CurrencyCode>,
    ) -> Result<SalaryAggregate, ApiError> {
        let clean_company_id = sanitize_text(company_id)?.to_lowercase();
        let target_curr = target_currency.unwrap_or(CurrencyCode::USD);
        let target_rate = target_curr.to_usd_rate();

        let clean_role = match role_filter {
            Some(r) if !r.trim().is_empty() => Some(sanitize_text(r)?.to_lowercase()),
            _ => None,
        };

        let reports = self.reports.read().unwrap();
        let matching: Vec<&SalaryReport> = reports
            .iter()
            .filter(|r| {
                if r.company_id != clean_company_id {
                    return false;
                }
                if let Some(ref role) = clean_role {
                    if r.job_title.to_lowercase() != *role {
                        return false;
                    }
                }
                true
            })
            .collect();

        let sample_size = matching.len();

        // D3: Privacy threshold check
        if sample_size < self.privacy_threshold {
            return Ok(SalaryAggregate {
                company_id: clean_company_id,
                job_title: clean_role,
                currency: target_curr.as_str().to_string(),
                sample_size,
                anonymized: true,
                message: Some(format!(
                    "Fewer than {} reports submitted for this role. Aggregate percentiles are suppressed to protect contributor anonymity.",
                    self.privacy_threshold
                )),
                stats: None,
            });
        }

        // Convert total compensation to target currency
        let converted_values: Vec<f64> = matching
            .iter()
            .map(|r| {
                let report_rate = r.currency.to_usd_rate();
                let in_usd = r.total_compensation * report_rate;
                in_usd / target_rate
            })
            .collect();

        let stats = compute_percentiles(&converted_values);

        Ok(SalaryAggregate {
            company_id: clean_company_id,
            job_title: clean_role,
            currency: target_curr.as_str().to_string(),
            sample_size,
            anonymized: false,
            message: None,
            stats,
        })
    }
}

pub static SALARY_STORE: LazyLock<SalaryStore> = LazyLock::new(SalaryStore::new);
