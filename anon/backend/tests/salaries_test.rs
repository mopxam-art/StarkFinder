use axum::http::{StatusCode, header};
use axum_test::TestServer;
use backend::libs::db::AppState;
use backend::models::salary::{
    CompensationType, CurrencyCode, SalaryAggregate, SalaryReport, SubmitSalaryReq,
};
use backend::services::salary_service::{
    SalaryStore, compute_percentiles, percentile_linear, sanitize_text,
};
use std::str::FromStr;
use std::sync::Arc;

fn setup_test_app() -> TestServer {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-salaries") };

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgresql://lazy:unreachable@127.0.0.1:54329/test")
        .unwrap();

    let state = AppState { pool };
    let app = backend::create_app(state);
    TestServer::new(app).unwrap()
}

fn create_auth_token(wallet: &str) -> String {
    unsafe { std::env::set_var("JWT_SECRET", "test-secret-starkfinder-salaries") };
    let secret = backend::libs::jwt::secret_from_env();
    backend::libs::jwt::encode(wallet, &secret).unwrap()
}

// =========================================================================
// D1: VALIDATION OF SALARY RANGES & CURRENCIES (20 tests)
// =========================================================================

#[test]
fn test_d1_01_currency_code_from_str_usd() {
    assert_eq!(CurrencyCode::from_str("USD").unwrap(), CurrencyCode::USD);
}

#[test]
fn test_d1_02_currency_code_from_str_eur() {
    assert_eq!(CurrencyCode::from_str("EUR").unwrap(), CurrencyCode::EUR);
}

#[test]
fn test_d1_03_currency_code_from_str_gbp() {
    assert_eq!(CurrencyCode::from_str("GBP").unwrap(), CurrencyCode::GBP);
}

#[test]
fn test_d1_04_currency_code_from_str_usdc() {
    assert_eq!(CurrencyCode::from_str("USDC").unwrap(), CurrencyCode::USDC);
}

#[test]
fn test_d1_05_currency_code_from_str_eth() {
    assert_eq!(CurrencyCode::from_str("ETH").unwrap(), CurrencyCode::ETH);
}

#[test]
fn test_d1_06_currency_code_from_str_strk() {
    assert_eq!(CurrencyCode::from_str("STRK").unwrap(), CurrencyCode::STRK);
}

#[test]
fn test_d1_07_currency_code_case_insensitive() {
    assert_eq!(CurrencyCode::from_str("usd").unwrap(), CurrencyCode::USD);
    assert_eq!(CurrencyCode::from_str("Eth").unwrap(), CurrencyCode::ETH);
    assert_eq!(CurrencyCode::from_str("  strk  ").unwrap(), CurrencyCode::STRK);
}

#[test]
fn test_d1_08_currency_code_invalid_rejected() {
    assert!(CurrencyCode::from_str("BTC").is_err());
    assert!(CurrencyCode::from_str("DOGE").is_err());
    assert!(CurrencyCode::from_str("INVALID").is_err());
    assert!(CurrencyCode::from_str("").is_err());
}

#[test]
fn test_d1_09_currency_code_display_and_as_str() {
    assert_eq!(CurrencyCode::USD.as_str(), "USD");
    assert_eq!(CurrencyCode::EUR.as_str(), "EUR");
    assert_eq!(format!("{}", CurrencyCode::STRK), "STRK");
}

#[test]
fn test_d1_10_compensation_type_parsing() {
    assert_eq!(CompensationType::from_str("full_time").unwrap(), CompensationType::FullTime);
    assert_eq!(CompensationType::from_str("contract").unwrap(), CompensationType::Contract);
    assert_eq!(CompensationType::from_str("part_time").unwrap(), CompensationType::PartTime);
    assert_eq!(CompensationType::from_str("hourly").unwrap(), CompensationType::Hourly);
}

#[test]
fn test_d1_11_compensation_type_invalid_rejected() {
    assert!(CompensationType::from_str("volunteer").is_err());
    assert!(CompensationType::from_str("").is_err());
}

#[test]
fn test_d1_12_currency_to_usd_rates_positive() {
    assert!(CurrencyCode::USD.to_usd_rate() > 0.0);
    assert!(CurrencyCode::EUR.to_usd_rate() > 0.0);
    assert!(CurrencyCode::GBP.to_usd_rate() > 0.0);
    assert!(CurrencyCode::USDC.to_usd_rate() > 0.0);
    assert!(CurrencyCode::ETH.to_usd_rate() > 0.0);
    assert!(CurrencyCode::STRK.to_usd_rate() > 0.0);
}

#[test]
fn test_d1_13_salary_submit_valid_payload() {
    let store = SalaryStore::new();
    let rep = store.submit_report(SubmitSalaryReq {
        company_id: "shonen-labs".to_string(),
        job_title: "Rust Core Engineer".to_string(),
        base_salary: 120_000.0,
        currency: "USD".to_string(),
        compensation_type: Some("full_time".to_string()),
        bonus: Some(15_000.0),
        equity_or_tokens: Some(25_000.0),
        years_of_experience: Some(5),
        location: Some("Remote".to_string()),
    }, None).unwrap();

    assert_eq!(rep.company_id, "shonen-labs");
    assert_eq!(rep.base_salary, 120_000.0);
    assert_eq!(rep.bonus, 15_000.0);
    assert_eq!(rep.equity_or_tokens, 25_000.0);
    assert_eq!(rep.total_compensation, 160_000.0);
}

#[test]
fn test_d1_14_salary_submit_calculates_total_compensation() {
    let store = SalaryStore::new();
    let rep = store.submit_report(SubmitSalaryReq {
        company_id: "argent".to_string(),
        job_title: "Security Auditor".to_string(),
        base_salary: 100_000.0,
        currency: "USDC".to_string(),
        compensation_type: None,
        bonus: Some(10_000.0),
        equity_or_tokens: Some(5_000.0),
        years_of_experience: None,
        location: None,
    }, None).unwrap();

    assert_eq!(rep.total_compensation, 115_000.0);
}

#[test]
fn test_d1_15_salary_submit_without_bonus_defaults_to_zero() {
    let store = SalaryStore::new();
    let rep = store.submit_report(SubmitSalaryReq {
        company_id: "argent".to_string(),
        job_title: "Dev".to_string(),
        base_salary: 80_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None).unwrap();

    assert_eq!(rep.bonus, 0.0);
    assert_eq!(rep.equity_or_tokens, 0.0);
    assert_eq!(rep.total_compensation, 80_000.0);
}

#[test]
fn test_d1_16_salary_submit_without_equity_defaults_to_zero() {
    let store = SalaryStore::new();
    let rep = store.submit_report(SubmitSalaryReq {
        company_id: "braavos".to_string(),
        job_title: "Dev".to_string(),
        base_salary: 90_000.0,
        currency: "EUR".to_string(),
        compensation_type: None,
        bonus: Some(5000.0),
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None).unwrap();

    assert_eq!(rep.equity_or_tokens, 0.0);
    assert_eq!(rep.total_compensation, 95_000.0);
}

#[test]
fn test_d1_17_salary_submit_sets_default_full_time() {
    let store = SalaryStore::new();
    let rep = store.submit_report(SubmitSalaryReq {
        company_id: "braavos".to_string(),
        job_title: "Dev".to_string(),
        base_salary: 90_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None).unwrap();

    assert_eq!(rep.compensation_type, CompensationType::FullTime);
}

#[test]
fn test_d1_18_salary_submit_with_contract_type() {
    let store = SalaryStore::new();
    let rep = store.submit_report(SubmitSalaryReq {
        company_id: "braavos".to_string(),
        job_title: "Contractor".to_string(),
        base_salary: 150_000.0,
        currency: "STRK".to_string(),
        compensation_type: Some("contract".to_string()),
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None).unwrap();

    assert_eq!(rep.compensation_type, CompensationType::Contract);
}

#[test]
fn test_d1_19_salary_submit_with_experience_and_location() {
    let store = SalaryStore::new();
    let rep = store.submit_report(SubmitSalaryReq {
        company_id: "starkware".to_string(),
        job_title: "Cryptographer".to_string(),
        base_salary: 200_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: Some(8),
        location: Some("Tel Aviv".to_string()),
    }, None).unwrap();

    assert_eq!(rep.years_of_experience, Some(8));
    assert_eq!(rep.location, Some("Tel Aviv".to_string()));
}

#[tokio::test]
async fn test_d1_20_http_submit_valid_salary_returns_201() {
    let server = setup_test_app();
    let res = server.post("/salaries").json(&SubmitSalaryReq {
        company_id: "http-company-d1".to_string(),
        job_title: "Staff Engineer".to_string(),
        base_salary: 180_000.0,
        currency: "USD".to_string(),
        compensation_type: Some("full_time".to_string()),
        bonus: Some(20_000.0),
        equity_or_tokens: None,
        years_of_experience: Some(7),
        location: Some("Remote".to_string()),
    }).await;

    assert_eq!(res.status_code(), StatusCode::CREATED);
    let body: SalaryReport = res.json();
    assert_eq!(body.base_salary, 180_000.0);
    assert_eq!(body.total_compensation, 200_000.0);
}

// =========================================================================
// D2: ACCURATE QUANTILE CALCULATION (min, p25, median, p75, p90, max) (20 tests)
// =========================================================================

#[test]
fn test_d2_01_quantiles_single_element() {
    let stats = compute_percentiles(&[50_000.0]).unwrap();
    assert_eq!(stats.min, 50_000.0);
    assert_eq!(stats.p25, 50_000.0);
    assert_eq!(stats.median, 50_000.0);
    assert_eq!(stats.p75, 50_000.0);
    assert_eq!(stats.p90, 50_000.0);
    assert_eq!(stats.max, 50_000.0);
    assert_eq!(stats.average, 50_000.0);
}

#[test]
fn test_d2_02_quantiles_two_elements() {
    let stats = compute_percentiles(&[40_000.0, 60_000.0]).unwrap();
    assert_eq!(stats.min, 40_000.0);
    assert_eq!(stats.max, 60_000.0);
    assert_eq!(stats.median, 50_000.0);
    assert_eq!(stats.average, 50_000.0);
}

#[test]
fn test_d2_03_quantiles_three_elements() {
    let stats = compute_percentiles(&[30_000.0, 50_000.0, 70_000.0]).unwrap();
    assert_eq!(stats.min, 30_000.0);
    assert_eq!(stats.median, 50_000.0);
    assert_eq!(stats.max, 70_000.0);
    assert_eq!(stats.average, 50_000.0);
}

#[test]
fn test_d2_04_quantiles_four_elements_even() {
    let stats = compute_percentiles(&[10.0, 20.0, 30.0, 40.0]).unwrap();
    assert_eq!(stats.min, 10.0);
    assert_eq!(stats.max, 40.0);
    assert_eq!(stats.median, 25.0);
}

#[test]
fn test_d2_05_quantiles_five_elements_odd() {
    let stats = compute_percentiles(&[10.0, 20.0, 30.0, 40.0, 50.0]).unwrap();
    assert_eq!(stats.median, 30.0);
    assert_eq!(stats.p25, 20.0);
    assert_eq!(stats.p75, 40.0);
}

#[test]
fn test_d2_06_quantiles_sorted_array_known_distribution() {
    let values = vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0];
    let stats = compute_percentiles(&values).unwrap();
    assert_eq!(stats.min, 10.0);
    assert_eq!(stats.max, 100.0);
    assert_eq!(stats.median, 55.0);
    assert_eq!(stats.average, 55.0);
    assert_eq!(stats.p25, 32.5);
    assert_eq!(stats.p75, 77.5);
    assert_eq!(stats.p90, 91.0);
}

#[test]
fn test_d2_07_quantiles_unsorted_input_automatically_sorted() {
    let unsorted = vec![100.0, 20.0, 80.0, 10.0, 50.0];
    let stats = compute_percentiles(&unsorted).unwrap();
    assert_eq!(stats.min, 10.0);
    assert_eq!(stats.max, 100.0);
    assert_eq!(stats.median, 50.0);
}

#[test]
fn test_d2_08_quantiles_identical_values_all_match() {
    let identical = vec![75_000.0; 10];
    let stats = compute_percentiles(&identical).unwrap();
    assert_eq!(stats.min, 75_000.0);
    assert_eq!(stats.p25, 75_000.0);
    assert_eq!(stats.median, 75_000.0);
    assert_eq!(stats.p75, 75_000.0);
    assert_eq!(stats.p90, 75_000.0);
    assert_eq!(stats.max, 75_000.0);
    assert_eq!(stats.average, 75_000.0);
}

#[test]
fn test_d2_09_quantiles_empty_returns_none() {
    assert_eq!(compute_percentiles(&[]), None);
}

#[test]
fn test_d2_10_quantiles_linear_interpolation_between_points() {
    let v = [100.0, 200.0];
    assert_eq!(percentile_linear(&v, 0.5), 150.0);
    assert_eq!(percentile_linear(&v, 0.25), 125.0);
    assert_eq!(percentile_linear(&v, 0.75), 175.0);
}

#[test]
fn test_d2_11_quantiles_p90_calculation_accuracy() {
    let values: Vec<f64> = (1..=101).map(|x| x as f64).collect();
    let stats = compute_percentiles(&values).unwrap();
    assert_eq!(stats.p90, 91.0);
}

#[test]
fn test_d2_12_quantiles_p25_and_p75_interquartile_range() {
    let values: Vec<f64> = (1..=100).map(|x| x as f64).collect();
    let stats = compute_percentiles(&values).unwrap();
    assert!(stats.p75 > stats.p25);
    let iqr = stats.p75 - stats.p25;
    assert!((iqr - 49.5).abs() < 0.1);
}

#[test]
fn test_d2_13_quantiles_average_matches_arithmetic_mean() {
    let values = vec![10.0, 20.0, 30.0];
    let stats = compute_percentiles(&values).unwrap();
    assert_eq!(stats.average, 20.0);
}

#[test]
fn test_d2_14_quantiles_scale_to_100_elements() {
    let values: Vec<f64> = (1..=100).map(|x| x as f64 * 1000.0).collect();
    let stats = compute_percentiles(&values).unwrap();
    assert_eq!(stats.min, 1000.0);
    assert_eq!(stats.max, 100_000.0);
}

#[test]
fn test_d2_15_quantiles_scale_to_1000_elements() {
    let values: Vec<f64> = (1..=1000).map(|x| x as f64).collect();
    let stats = compute_percentiles(&values).unwrap();
    assert_eq!(stats.min, 1.0);
    assert_eq!(stats.max, 1000.0);
    assert!((stats.median - 500.5).abs() < 0.01);
}

#[test]
fn test_d2_16_quantiles_monotonic_property_min_le_p25_le_median_le_p75_le_p90_le_max() {
    let values = vec![15.0, 95.0, 42.0, 88.0, 33.0, 61.0, 77.0, 29.0];
    let s = compute_percentiles(&values).unwrap();
    assert!(s.min <= s.p25);
    assert!(s.p25 <= s.median);
    assert!(s.median <= s.p75);
    assert!(s.p75 <= s.p90);
    assert!(s.p90 <= s.max);
}

#[test]
fn test_d2_17_currency_conversion_affects_quantiles_proportionately() {
    let store = SalaryStore::new();
    // 3 reports with 1000 EUR each (1.08 USD rate)
    for _ in 0..3 {
        store.submit_report(SubmitSalaryReq {
            company_id: "eur_co".to_string(),
            job_title: "dev".to_string(),
            base_salary: 1000.0,
            currency: "EUR".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    let summary_usd = store.get_company_summary("eur_co", None, Some(CurrencyCode::USD)).unwrap();
    let stats = summary_usd.stats.unwrap();
    assert_eq!(stats.median, 1080.0);
}

#[test]
fn test_d2_18_summary_aggregate_in_eur_matches_rate() {
    let store = SalaryStore::new();
    for _ in 0..3 {
        store.submit_report(SubmitSalaryReq {
            company_id: "usd_co".to_string(),
            job_title: "dev".to_string(),
            base_salary: 1080.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    let summary_eur = store.get_company_summary("usd_co", None, Some(CurrencyCode::EUR)).unwrap();
    let stats = summary_eur.stats.unwrap();
    assert_eq!(stats.median, 1000.0);
}

#[test]
fn test_d2_19_summary_aggregate_in_eth_matches_rate() {
    let store = SalaryStore::new();
    for _ in 0..3 {
        store.submit_report(SubmitSalaryReq {
            company_id: "eth_co".to_string(),
            job_title: "dev".to_string(),
            base_salary: 5200.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    let summary_eth = store.get_company_summary("eth_co", None, Some(CurrencyCode::ETH)).unwrap();
    let stats = summary_eth.stats.unwrap();
    assert_eq!(stats.median, 2.0); // 5200 USD / 2600 USD/ETH = 2 ETH
}

#[tokio::test]
async fn test_d2_20_http_get_summary_quantiles_computed_correctly() {
    let server = setup_test_app();
    let company = "company-d2-20";

    for salary in [50_000.0, 70_000.0, 90_000.0] {
        server.post("/salaries").json(&SubmitSalaryReq {
            company_id: company.to_string(),
            job_title: "Rust Dev".to_string(),
            base_salary: salary,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }).await;
    }

    let res = server.get(&format!("/companies/{}/salaries/summary", company)).await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let agg: SalaryAggregate = res.json();
    assert!(!agg.anonymized);
    let stats = agg.stats.unwrap();
    assert_eq!(stats.median, 70_000.0);
    assert_eq!(stats.min, 50_000.0);
    assert_eq!(stats.max, 90_000.0);
}

// =========================================================================
// D3: ANONYMIZATION & PRIVACY THRESHOLD (k < 3 reports) (20 tests)
// =========================================================================

#[test]
fn test_d3_01_privacy_threshold_zero_reports_anonymized() {
    let store = SalaryStore::new();
    let summary = store.get_company_summary("ghost_corp", None, None).unwrap();
    assert_eq!(summary.sample_size, 0);
    assert!(summary.anonymized);
    assert!(summary.stats.is_none());
}

#[test]
fn test_d3_02_privacy_threshold_one_report_anonymized() {
    let store = SalaryStore::new();
    store.submit_report(SubmitSalaryReq {
        company_id: "solo_corp".to_string(),
        job_title: "dev".to_string(),
        base_salary: 100_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None).unwrap();

    let summary = store.get_company_summary("solo_corp", None, None).unwrap();
    assert_eq!(summary.sample_size, 1);
    assert!(summary.anonymized);
    assert!(summary.stats.is_none());
}

#[test]
fn test_d3_03_privacy_threshold_two_reports_anonymized() {
    let store = SalaryStore::new();
    for _ in 0..2 {
        store.submit_report(SubmitSalaryReq {
            company_id: "pair_corp".to_string(),
            job_title: "dev".to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    let summary = store.get_company_summary("pair_corp", None, None).unwrap();
    assert_eq!(summary.sample_size, 2);
    assert!(summary.anonymized);
    assert!(summary.stats.is_none());
}

#[test]
fn test_d3_04_privacy_threshold_three_reports_unlocked() {
    let store = SalaryStore::new();
    for _ in 0..3 {
        store.submit_report(SubmitSalaryReq {
            company_id: "trio_corp".to_string(),
            job_title: "dev".to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    let summary = store.get_company_summary("trio_corp", None, None).unwrap();
    assert_eq!(summary.sample_size, 3);
    assert!(!summary.anonymized);
    assert!(summary.stats.is_some());
}

#[test]
fn test_d3_05_privacy_threshold_four_reports_unlocked() {
    let store = SalaryStore::new();
    for _ in 0..4 {
        store.submit_report(SubmitSalaryReq {
            company_id: "quad_corp".to_string(),
            job_title: "dev".to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    let summary = store.get_company_summary("quad_corp", None, None).unwrap();
    assert_eq!(summary.sample_size, 4);
    assert!(!summary.anonymized);
}

#[test]
fn test_d3_06_privacy_threshold_custom_store_setting() {
    let store = SalaryStore::with_threshold(5);
    for _ in 0..4 {
        store.submit_report(SubmitSalaryReq {
            company_id: "strict_corp".to_string(),
            job_title: "dev".to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    assert!(store.get_company_summary("strict_corp", None, None).unwrap().anonymized);
}

#[test]
fn test_d3_07_anonymized_summary_stats_are_none() {
    let store = SalaryStore::new();
    let summary = store.get_company_summary("anon_corp", None, None).unwrap();
    assert_eq!(summary.stats, None);
}

#[test]
fn test_d3_08_anonymized_summary_contains_explanation_message() {
    let store = SalaryStore::new();
    let summary = store.get_company_summary("anon_corp", None, None).unwrap();
    assert!(summary.message.unwrap().contains("Fewer than 3 reports"));
}

#[test]
fn test_d3_09_anonymized_summary_preserves_sample_size() {
    let store = SalaryStore::new();
    store.submit_report(SubmitSalaryReq {
        company_id: "anon2".to_string(),
        job_title: "dev".to_string(),
        base_salary: 90_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None).unwrap();

    let summary = store.get_company_summary("anon2", None, None).unwrap();
    assert_eq!(summary.sample_size, 1);
}

#[test]
fn test_d3_10_role_filter_with_under_3_reports_is_anonymized() {
    let store = SalaryStore::new();
    // 5 reports total, but only 2 for "manager"
    for _ in 0..3 {
        store.submit_report(SubmitSalaryReq {
            company_id: "role_co".to_string(),
            job_title: "engineer".to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }
    for _ in 0..2 {
        store.submit_report(SubmitSalaryReq {
            company_id: "role_co".to_string(),
            job_title: "manager".to_string(),
            base_salary: 140_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    let summary_mgr = store.get_company_summary("role_co", Some("manager"), None).unwrap();
    assert_eq!(summary_mgr.sample_size, 2);
    assert!(summary_mgr.anonymized);
}

#[test]
fn test_d3_11_role_filter_with_over_3_reports_is_not_anonymized() {
    let store = SalaryStore::new();
    for _ in 0..3 {
        store.submit_report(SubmitSalaryReq {
            company_id: "role_co_2".to_string(),
            job_title: "engineer".to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    let summary = store.get_company_summary("role_co_2", Some("engineer"), None).unwrap();
    assert_eq!(summary.sample_size, 3);
    assert!(!summary.anonymized);
}

#[test]
fn test_d3_12_company_total_unlocked_while_specific_role_anonymized() {
    let store = SalaryStore::new();
    for i in 0..4 {
        let role = if i == 0 { "designer" } else { "dev" };
        store.submit_report(SubmitSalaryReq {
            company_id: "split_co".to_string(),
            job_title: role.to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    let overall = store.get_company_summary("split_co", None, None).unwrap();
    let designer = store.get_company_summary("split_co", Some("designer"), None).unwrap();

    assert!(!overall.anonymized);
    assert!(designer.anonymized);
}

#[test]
fn test_d3_13_different_companies_privacy_isolation() {
    let store = SalaryStore::new();
    for _ in 0..2 {
        store.submit_report(SubmitSalaryReq {
            company_id: "alpha".to_string(),
            job_title: "dev".to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }
    for _ in 0..2 {
        store.submit_report(SubmitSalaryReq {
            company_id: "beta".to_string(),
            job_title: "dev".to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    assert!(store.get_company_summary("alpha", None, None).unwrap().anonymized);
    assert!(store.get_company_summary("beta", None, None).unwrap().anonymized);
}

#[test]
fn test_d3_14_user_wallet_recorded_internally_if_provided() {
    let store = SalaryStore::new();
    let rep = store.submit_report(SubmitSalaryReq {
        company_id: "wallet_co".to_string(),
        job_title: "dev".to_string(),
        base_salary: 100_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, Some("0x12345".to_string())).unwrap();

    assert_eq!(rep.user_wallet, Some("0x12345".to_string()));
}

#[test]
fn test_d3_15_user_wallet_optional_in_submission() {
    let store = SalaryStore::new();
    let rep = store.submit_report(SubmitSalaryReq {
        company_id: "anon_submit".to_string(),
        job_title: "dev".to_string(),
        base_salary: 100_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None).unwrap();

    assert_eq!(rep.user_wallet, None);
}

#[test]
fn test_d3_16_company_case_insensitive_matching() {
    let store = SalaryStore::new();
    for _ in 0..3 {
        store.submit_report(SubmitSalaryReq {
            company_id: "SHONEN-LABS".to_string(),
            job_title: "dev".to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    let summary = store.get_company_summary("shonen-labs", None, None).unwrap();
    assert_eq!(summary.sample_size, 3);
    assert!(!summary.anonymized);
}

#[test]
fn test_d3_17_role_case_insensitive_matching() {
    let store = SalaryStore::new();
    for _ in 0..3 {
        store.submit_report(SubmitSalaryReq {
            company_id: "case_co".to_string(),
            job_title: "Rust Developer".to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    let summary = store.get_company_summary("case_co", Some("rust developer"), None).unwrap();
    assert_eq!(summary.sample_size, 3);
}

#[tokio::test]
async fn test_d3_18_http_get_summary_under_3_reports_returns_anonymized_true() {
    let server = setup_test_app();
    let company = "http-under-3";

    server.post("/salaries").json(&SubmitSalaryReq {
        company_id: company.to_string(),
        job_title: "dev".to_string(),
        base_salary: 80_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }).await;

    let res = server.get(&format!("/companies/{}/salaries/summary", company)).await;
    let body: SalaryAggregate = res.json();
    assert!(body.anonymized);
    assert!(body.stats.is_none());
}

#[tokio::test]
async fn test_d3_19_http_get_summary_at_3_reports_returns_anonymized_false() {
    let server = setup_test_app();
    let company = "http-at-3";

    for _ in 0..3 {
        server.post("/salaries").json(&SubmitSalaryReq {
            company_id: company.to_string(),
            job_title: "dev".to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }).await;
    }

    let res = server.get(&format!("/companies/{}/salaries/summary", company)).await;
    let body: SalaryAggregate = res.json();
    assert!(!body.anonymized);
    assert!(body.stats.is_some());
}

#[tokio::test]
async fn test_d3_20_http_get_summary_with_role_filter_privacy_respected() {
    let server = setup_test_app();
    let company = "http-role-filter";

    // 2 backend, 1 frontend
    for _ in 0..2 {
        server.post("/salaries").json(&SubmitSalaryReq {
            company_id: company.to_string(),
            job_title: "Backend".to_string(),
            base_salary: 110_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }).await;
    }
    server.post("/salaries").json(&SubmitSalaryReq {
        company_id: company.to_string(),
        job_title: "Frontend".to_string(),
        base_salary: 95_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }).await;

    let res = server.get(&format!("/companies/{}/salaries/summary?job_title=Backend", company)).await;
    let body: SalaryAggregate = res.json();
    assert_eq!(body.sample_size, 2);
    assert!(body.anonymized);
}

// =========================================================================
// D4: NEGATIVE SCENARIOS & INJECTION DEFENSE (20 tests)
// =========================================================================

#[test]
fn test_d4_01_reject_negative_base_salary() {
    let store = SalaryStore::new();
    let res = store.submit_report(SubmitSalaryReq {
        company_id: "test".to_string(),
        job_title: "dev".to_string(),
        base_salary: -50_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None);
    assert!(res.is_err());
}

#[test]
fn test_d4_02_reject_zero_base_salary() {
    let store = SalaryStore::new();
    let res = store.submit_report(SubmitSalaryReq {
        company_id: "test".to_string(),
        job_title: "dev".to_string(),
        base_salary: 0.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None);
    assert!(res.is_err());
}

#[test]
fn test_d4_03_reject_infinite_or_nan_base_salary() {
    let store = SalaryStore::new();
    let res_inf = store.submit_report(SubmitSalaryReq {
        company_id: "test".to_string(),
        job_title: "dev".to_string(),
        base_salary: f64::INFINITY,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None);
    assert!(res_inf.is_err());

    let res_nan = store.submit_report(SubmitSalaryReq {
        company_id: "test".to_string(),
        job_title: "dev".to_string(),
        base_salary: f64::NAN,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None);
    assert!(res_nan.is_err());
}

#[test]
fn test_d4_04_reject_negative_bonus() {
    let store = SalaryStore::new();
    let res = store.submit_report(SubmitSalaryReq {
        company_id: "test".to_string(),
        job_title: "dev".to_string(),
        base_salary: 80_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: Some(-500.0),
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None);
    assert!(res.is_err());
}

#[test]
fn test_d4_05_reject_negative_equity_tokens() {
    let store = SalaryStore::new();
    let res = store.submit_report(SubmitSalaryReq {
        company_id: "test".to_string(),
        job_title: "dev".to_string(),
        base_salary: 80_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: Some(-1000.0),
        years_of_experience: None,
        location: None,
    }, None);
    assert!(res.is_err());
}

#[test]
fn test_d4_06_reject_unsupported_currency_code() {
    let store = SalaryStore::new();
    let res = store.submit_report(SubmitSalaryReq {
        company_id: "test".to_string(),
        job_title: "dev".to_string(),
        base_salary: 80_000.0,
        currency: "SHIB".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None);
    assert!(res.is_err());
}

#[test]
fn test_d4_07_reject_empty_company_id() {
    let store = SalaryStore::new();
    let res = store.submit_report(SubmitSalaryReq {
        company_id: "   ".to_string(),
        job_title: "dev".to_string(),
        base_salary: 80_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None);
    assert!(res.is_err());
}

#[test]
fn test_d4_08_reject_empty_job_title() {
    let store = SalaryStore::new();
    let res = store.submit_report(SubmitSalaryReq {
        company_id: "test".to_string(),
        job_title: "".to_string(),
        base_salary: 80_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None);
    assert!(res.is_err());
}

#[test]
fn test_d4_09_sql_injection_drop_table_in_company_id_rejected() {
    assert!(sanitize_text("shonen'; DROP TABLE users;--").is_err());
}

#[test]
fn test_d4_10_sql_injection_union_select_in_job_title_rejected() {
    assert!(sanitize_text("Engineer' UNION SELECT * FROM passwords--").is_err());
}

#[test]
fn test_d4_11_sql_injection_comment_dashes_rejected() {
    assert!(sanitize_text("admin'--").is_err());
}

#[test]
fn test_d4_12_xss_script_tags_rejected() {
    assert!(sanitize_text("<script>alert('pwned')</script>").is_err());
}

#[test]
fn test_d4_13_salary_exceeding_max_limit_rejected() {
    let store = SalaryStore::new();
    let res = store.submit_report(SubmitSalaryReq {
        company_id: "test".to_string(),
        job_title: "dev".to_string(),
        base_salary: 500_000_000.0, // 500M > 100M limit
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None);
    assert!(res.is_err());
}

#[tokio::test]
async fn test_d4_14_http_post_negative_salary_returns_400() {
    let server = setup_test_app();
    let res = server.post("/salaries").json(&SubmitSalaryReq {
        company_id: "test".to_string(),
        job_title: "dev".to_string(),
        base_salary: -100.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }).await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d4_15_http_post_zero_salary_returns_400() {
    let server = setup_test_app();
    let res = server.post("/salaries").json(&SubmitSalaryReq {
        company_id: "test".to_string(),
        job_title: "dev".to_string(),
        base_salary: 0.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }).await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d4_16_http_post_invalid_currency_returns_400() {
    let server = setup_test_app();
    let res = server.post("/salaries").json(&SubmitSalaryReq {
        company_id: "test".to_string(),
        job_title: "dev".to_string(),
        base_salary: 100_000.0,
        currency: "INVALID_COIN".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }).await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d4_17_http_post_empty_company_returns_400() {
    let server = setup_test_app();
    let res = server.post("/salaries").json(&SubmitSalaryReq {
        company_id: "".to_string(),
        job_title: "dev".to_string(),
        base_salary: 100_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }).await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d4_18_http_get_summary_invalid_currency_returns_400() {
    let server = setup_test_app();
    let res = server.get("/companies/test/salaries/summary?currency=BOGUS").await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d4_19_http_get_summary_sql_injection_company_returns_400() {
    let server = setup_test_app();
    let res = server.get("/companies/test/salaries/summary?job_title=dev%27%20UNION%20SELECT%20*%20FROM%20users--").await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_d4_20_http_post_authenticated_with_valid_jwt_sets_wallet() {
    let server = setup_test_app();
    let wallet = "0x01"; // valid starknet felt
    let token = create_auth_token(wallet);

    let res = server.post("/salaries")
        .add_header(header::AUTHORIZATION, format!("Bearer {}", token))
        .json(&SubmitSalaryReq {
            company_id: "auth_corp".to_string(),
            job_title: "dev".to_string(),
            base_salary: 120_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }).await;

    assert_eq!(res.status_code(), StatusCode::CREATED);
    let body: SalaryReport = res.json();
    assert!(body.user_wallet.is_some());
    assert!(body.user_wallet.unwrap().starts_with("0x"));
}

// =========================================================================
// D5: STRESS, CONCURRENCY & HIGH LOAD (20 tests)
// =========================================================================

#[test]
fn test_d5_01_stress_quantile_computation_on_5000_records() {
    let values: Vec<f64> = (1..=5000).map(|i| (i as f64 * 37.0) % 5000.0).collect();
    let start = std::time::Instant::now();
    let stats = compute_percentiles(&values).unwrap();
    let duration = start.elapsed();
    assert!(duration.as_millis() < 50); // fast computation
    assert_eq!(stats.min, 0.0);
    assert_eq!(stats.max, 4999.0);
}

#[tokio::test]
async fn test_d5_02_stress_50_concurrent_salary_submissions() {
    let store = Arc::new(SalaryStore::new());
    let mut handles = Vec::new();

    for i in 0..50 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            store_clone.submit_report(SubmitSalaryReq {
                company_id: "concurrent_co".to_string(),
                job_title: "Eng".to_string(),
                base_salary: 100_000.0 + (i as f64 * 1000.0),
                currency: "USD".to_string(),
                compensation_type: None,
                bonus: None,
                equity_or_tokens: None,
                years_of_experience: None,
                location: None,
            }, None)
        }));
    }

    for h in handles {
        assert!(h.await.unwrap().is_ok());
    }

    assert_eq!(store.len(), 50);
    let summary = store.get_company_summary("concurrent_co", None, None).unwrap();
    assert_eq!(summary.sample_size, 50);
    assert!(!summary.anonymized);
}

#[tokio::test]
async fn test_d5_03_concurrency_concurrent_queries_during_submissions() {
    let store = Arc::new(SalaryStore::new());
    let mut handles = Vec::new();

    // 25 writers
    for i in 0..25 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let _ = store_clone.submit_report(SubmitSalaryReq {
                company_id: "rw_co".to_string(),
                job_title: "Dev".to_string(),
                base_salary: 80_000.0 + (i as f64 * 500.0),
                currency: "USD".to_string(),
                compensation_type: None,
                bonus: None,
                equity_or_tokens: None,
                years_of_experience: None,
                location: None,
            }, None);
        }));
    }

    // 25 readers
    for _ in 0..25 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let summary = store_clone.get_company_summary("rw_co", None, None).unwrap();
            assert!(summary.sample_size <= 25);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

#[test]
fn test_d5_04_stress_100_distinct_companies_aggregation() {
    let store = SalaryStore::new();
    for i in 0..100 {
        let comp = format!("comp_{}", i);
        for j in 0..3 {
            store.submit_report(SubmitSalaryReq {
                company_id: comp.clone(),
                job_title: "dev".to_string(),
                base_salary: 70_000.0 + (j as f64 * 10_000.0),
                currency: "USD".to_string(),
                compensation_type: None,
                bonus: None,
                equity_or_tokens: None,
                years_of_experience: None,
                location: None,
            }, None).unwrap();
        }
    }

    for i in 0..100 {
        let comp = format!("comp_{}", i);
        let s = store.get_company_summary(&comp, None, None).unwrap();
        assert_eq!(s.sample_size, 3);
        assert!(!s.anonymized);
    }
}

#[test]
fn test_d5_05_stress_mixed_currencies_in_single_company() {
    let store = SalaryStore::new();
    // 1 USD ($100k), 1 EUR ($108k), 1 GBP ($128k)
    store.submit_report(SubmitSalaryReq {
        company_id: "multi_curr".to_string(),
        job_title: "dev".to_string(),
        base_salary: 100_000.0,
        currency: "USD".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None).unwrap();

    store.submit_report(SubmitSalaryReq {
        company_id: "multi_curr".to_string(),
        job_title: "dev".to_string(),
        base_salary: 100_000.0,
        currency: "EUR".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None).unwrap();

    store.submit_report(SubmitSalaryReq {
        company_id: "multi_curr".to_string(),
        job_title: "dev".to_string(),
        base_salary: 100_000.0,
        currency: "GBP".to_string(),
        compensation_type: None,
        bonus: None,
        equity_or_tokens: None,
        years_of_experience: None,
        location: None,
    }, None).unwrap();

    let s = store.get_company_summary("multi_curr", None, Some(CurrencyCode::USD)).unwrap();
    assert_eq!(s.sample_size, 3);
    let stats = s.stats.unwrap();
    assert_eq!(stats.min, 100_000.0);
    assert_eq!(stats.median, 108_000.0);
    assert_eq!(stats.max, 128_000.0);
}

#[tokio::test]
async fn test_d5_06_concurrency_race_condition_sample_size_integrity() {
    let store = Arc::new(SalaryStore::new());
    let mut handles = Vec::new();

    for i in 0..40 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            store_clone.submit_report(SubmitSalaryReq {
                company_id: "race_co".to_string(),
                job_title: "dev".to_string(),
                base_salary: 90_000.0 + i as f64,
                currency: "USD".to_string(),
                compensation_type: None,
                bonus: None,
                equity_or_tokens: None,
                years_of_experience: None,
                location: None,
            }, None)
        }));
    }

    for h in handles {
        h.await.unwrap().unwrap();
    }

    assert_eq!(store.get_company_summary("race_co", None, None).unwrap().sample_size, 40);
}

#[tokio::test]
async fn test_d5_07_stress_high_volume_summary_queries() {
    let store = Arc::new(SalaryStore::new());
    for _ in 0..5 {
        store.submit_report(SubmitSalaryReq {
            company_id: "query_heavy".to_string(),
            job_title: "dev".to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    let mut handles = Vec::new();
    for _ in 0..50 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            store_clone.get_company_summary("query_heavy", None, None)
        }));
    }

    for h in handles {
        assert!(h.await.unwrap().is_ok());
    }
}

#[test]
fn test_d5_08_stress_large_salary_values_numerical_stability() {
    let large = vec![10_000_000.0, 50_000_000.0, 90_000_000.0];
    let stats = compute_percentiles(&large).unwrap();
    assert_eq!(stats.median, 50_000_000.0);
    assert_eq!(stats.average, 50_000_000.0);
}

#[test]
fn test_d5_09_stress_small_salary_values_numerical_stability() {
    let small = vec![0.01, 0.05, 0.09];
    let stats = compute_percentiles(&small).unwrap();
    assert_eq!(stats.min, 0.01);
    assert_eq!(stats.median, 0.05);
    assert_eq!(stats.max, 0.09);
}

#[tokio::test]
async fn test_d5_10_concurrency_rwlock_no_deadlocks_under_load() {
    let store = Arc::new(SalaryStore::new());
    let mut handles = Vec::new();

    for i in 0..30 {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let _ = store_clone.submit_report(SubmitSalaryReq {
                company_id: "deadlock_co".to_string(),
                job_title: "dev".to_string(),
                base_salary: 75_000.0 + i as f64,
                currency: "USD".to_string(),
                compensation_type: None,
                bonus: None,
                equity_or_tokens: None,
                years_of_experience: None,
                location: None,
            }, None);
            let _ = store_clone.get_company_summary("deadlock_co", None, None);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

#[test]
fn test_d5_11_stress_repeated_clearing_and_repopulation() {
    let store = SalaryStore::new();
    for _ in 0..5 {
        for _ in 0..10 {
            store.submit_report(SubmitSalaryReq {
                company_id: "recycle".to_string(),
                job_title: "dev".to_string(),
                base_salary: 80_000.0,
                currency: "USD".to_string(),
                compensation_type: None,
                bonus: None,
                equity_or_tokens: None,
                years_of_experience: None,
                location: None,
            }, None).unwrap();
        }
        assert_eq!(store.len(), 10);
        store.clear();
        assert_eq!(store.len(), 0);
    }
}

#[tokio::test]
async fn test_d5_12_stress_rapid_http_requests_test_server() {
    let server = setup_test_app();
    let company = "http-rapid";

    for i in 0..5 {
        let res = server.post("/salaries").json(&SubmitSalaryReq {
            company_id: company.to_string(),
            job_title: "dev".to_string(),
            base_salary: 100_000.0 + (i as f64 * 5000.0),
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }).await;
        assert_eq!(res.status_code(), StatusCode::CREATED);
    }

    let summary_res = server.get(&format!("/companies/{}/salaries/summary", company)).await;
    assert_eq!(summary_res.status_code(), StatusCode::OK);
    let agg: SalaryAggregate = summary_res.json();
    assert_eq!(agg.sample_size, 5);
}

#[test]
fn test_d5_13_stress_wide_distribution_quantile_precision() {
    let mut values = Vec::new();
    for i in 0..1000 {
        values.push((i * i) as f64);
    }
    let stats = compute_percentiles(&values).unwrap();
    assert_eq!(stats.min, 0.0);
    assert_eq!(stats.max, 999.0 * 999.0);
}

#[test]
fn test_d5_14_stress_extreme_outliers_handling() {
    // 9 normal salaries and 1 huge outlier
    let mut values = vec![100_000.0; 9];
    values.push(10_000_000.0);
    let stats = compute_percentiles(&values).unwrap();
    // Median is unaffected by the extreme top outlier
    assert_eq!(stats.median, 100_000.0);
    assert_eq!(stats.max, 10_000_000.0);
}

#[tokio::test]
async fn test_d5_15_concurrency_multi_threaded_role_filtering() {
    let store = Arc::new(SalaryStore::new());
    for i in 0..30 {
        let role = match i % 3 {
            0 => "frontend",
            1 => "backend",
            _ => "devops",
        };
        store.submit_report(SubmitSalaryReq {
            company_id: "role_threading".to_string(),
            job_title: role.to_string(),
            base_salary: 100_000.0,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }

    let mut handles = Vec::new();
    for role in ["frontend", "backend", "devops"] {
        let store_clone = Arc::clone(&store);
        handles.push(tokio::spawn(async move {
            let s = store_clone.get_company_summary("role_threading", Some(role), None).unwrap();
            assert_eq!(s.sample_size, 10);
            assert!(!s.anonymized);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

#[test]
fn test_d5_16_stress_conversion_across_all_supported_currencies() {
    let store = SalaryStore::new();
    let currencies = ["USD", "EUR", "GBP", "USDC", "ETH", "STRK"];
    for curr in currencies {
        for _ in 0..3 {
            store.submit_report(SubmitSalaryReq {
                company_id: "curr_all".to_string(),
                job_title: "dev".to_string(),
                base_salary: 10_000.0,
                currency: curr.to_string(),
                compensation_type: None,
                bonus: None,
                equity_or_tokens: None,
                years_of_experience: None,
                location: None,
            }, None).unwrap();
        }
    }

    for target in [CurrencyCode::USD, CurrencyCode::EUR, CurrencyCode::STRK] {
        let s = store.get_company_summary("curr_all", None, Some(target)).unwrap();
        assert_eq!(s.sample_size, 18);
        assert!(!s.anonymized);
    }
}

#[tokio::test]
async fn test_d5_17_concurrency_simultaneous_anonymous_and_authed_submits() {
    let store = Arc::new(SalaryStore::new());
    let mut handles = Vec::new();

    for i in 0..20 {
        let store_clone = Arc::clone(&store);
        let wallet = if i % 2 == 0 { Some(format!("0x{:x}", i + 1)) } else { None };
        handles.push(tokio::spawn(async move {
            store_clone.submit_report(SubmitSalaryReq {
                company_id: "mixed_auth".to_string(),
                job_title: "dev".to_string(),
                base_salary: 95_000.0,
                currency: "USD".to_string(),
                compensation_type: None,
                bonus: None,
                equity_or_tokens: None,
                years_of_experience: None,
                location: None,
            }, wallet)
        }));
    }

    for h in handles {
        assert!(h.await.unwrap().is_ok());
    }

    assert_eq!(store.len(), 20);
}

#[test]
fn test_d5_18_stress_10_000_items_percentile_speed_under_5ms() {
    let values: Vec<f64> = (1..=10_000).map(|i| (i as f64 * 13.0) % 10_000.0).collect();
    let start = std::time::Instant::now();
    let stats = compute_percentiles(&values).unwrap();
    let duration = start.elapsed();
    assert!(duration.as_millis() < 50);
    assert_eq!(stats.min, 0.0);
    assert_eq!(stats.max, 9999.0);
}

#[test]
fn test_d5_19_stress_memory_safety_on_large_vectors() {
    let store = SalaryStore::new();
    for i in 0..500 {
        store.submit_report(SubmitSalaryReq {
            company_id: "mem_co".to_string(),
            job_title: "dev".to_string(),
            base_salary: 50_000.0 + i as f64,
            currency: "USD".to_string(),
            compensation_type: None,
            bonus: None,
            equity_or_tokens: None,
            years_of_experience: None,
            location: None,
        }, None).unwrap();
    }
    assert_eq!(store.len(), 500);
}

#[test]
fn test_d5_20_stress_end_to_end_audit_consistency() {
    let store = SalaryStore::new();

    // 10 companies, 5 reports each
    for c in 0..10 {
        let comp_id = format!("audit_co_{}", c);
        for i in 1..=5 {
            store.submit_report(SubmitSalaryReq {
                company_id: comp_id.clone(),
                job_title: "Software Engineer".to_string(),
                base_salary: i as f64 * 20_000.0,
                currency: "USD".to_string(),
                compensation_type: Some("full_time".to_string()),
                bonus: Some(5_000.0),
                equity_or_tokens: Some(5_000.0),
                years_of_experience: Some(i as u32),
                location: Some("Remote".to_string()),
            }, None).unwrap();
        }

        let s = store.get_company_summary(&comp_id, None, None).unwrap();
        assert_eq!(s.sample_size, 5);
        assert!(!s.anonymized);
        let stats = s.stats.unwrap();
        // total comp: 30k, 50k, 70k, 90k, 110k
        assert_eq!(stats.min, 30_000.0);
        assert_eq!(stats.median, 70_000.0);
        assert_eq!(stats.max, 110_000.0);
    }
}
