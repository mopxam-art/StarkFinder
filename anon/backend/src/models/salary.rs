use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum CurrencyCode {
    USD,
    EUR,
    GBP,
    USDC,
    ETH,
    STRK,
}

impl CurrencyCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            CurrencyCode::USD => "USD",
            CurrencyCode::EUR => "EUR",
            CurrencyCode::GBP => "GBP",
            CurrencyCode::USDC => "USDC",
            CurrencyCode::ETH => "ETH",
            CurrencyCode::STRK => "STRK",
        }
    }

    pub fn to_usd_rate(&self) -> f64 {
        match self {
            CurrencyCode::USD => 1.0,
            CurrencyCode::USDC => 1.0,
            CurrencyCode::EUR => 1.08,
            CurrencyCode::GBP => 1.28,
            CurrencyCode::ETH => 2600.0,
            CurrencyCode::STRK => 0.45,
        }
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for CurrencyCode {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_uppercase().as_str() {
            "USD" => Ok(CurrencyCode::USD),
            "EUR" => Ok(CurrencyCode::EUR),
            "GBP" => Ok(CurrencyCode::GBP),
            "USDC" => Ok(CurrencyCode::USDC),
            "ETH" => Ok(CurrencyCode::ETH),
            "STRK" => Ok(CurrencyCode::STRK),
            _ => Err("unsupported currency: must be USD, EUR, GBP, USDC, ETH, or STRK"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompensationType {
    #[default]
    FullTime,
    Contract,
    PartTime,
    Hourly,
}

impl FromStr for CompensationType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "full_time" | "fulltime" | "full-time" => Ok(CompensationType::FullTime),
            "contract" => Ok(CompensationType::Contract),
            "part_time" | "parttime" | "part-time" => Ok(CompensationType::PartTime),
            "hourly" => Ok(CompensationType::Hourly),
            _ => Err("invalid compensation type"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SubmitSalaryReq {
    pub company_id: String,
    pub job_title: String,
    pub base_salary: f64,
    pub currency: String,
    pub compensation_type: Option<String>,
    pub bonus: Option<f64>,
    pub equity_or_tokens: Option<f64>,
    pub years_of_experience: Option<u32>,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SalaryReport {
    pub id: String,
    pub company_id: String,
    pub job_title: String,
    pub base_salary: f64,
    pub currency: CurrencyCode,
    pub compensation_type: CompensationType,
    pub bonus: f64,
    pub equity_or_tokens: f64,
    pub total_compensation: f64,
    pub years_of_experience: Option<u32>,
    pub location: Option<String>,
    pub user_wallet: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PercentileStats {
    pub min: f64,
    pub p25: f64,
    pub median: f64,
    pub p75: f64,
    pub p90: f64,
    pub max: f64,
    pub average: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SalaryAggregate {
    pub company_id: String,
    pub job_title: Option<String>,
    pub currency: String,
    pub sample_size: usize,
    pub anonymized: bool,
    pub message: Option<String>,
    pub stats: Option<PercentileStats>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SalarySummaryQuery {
    pub job_title: Option<String>,
    pub currency: Option<String>,
}
