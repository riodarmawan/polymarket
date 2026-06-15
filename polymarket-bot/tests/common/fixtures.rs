use clap::Parser;
use polymarket_bot::analyzers::orderbook::{OrderBookAnalyzer, OrderBookSnapshot};
use polymarket_bot::api::clob::ClobClient;
use polymarket_bot::api::gamma::GammaClient;
use polymarket_bot::api::types::{Market, OrderBookLevel};
use polymarket_bot::cli::{Cli, Commands};
use polymarket_bot::collector::data_collector::DataCollector;
use polymarket_bot::config::Config;
use polymarket_bot::dashboard::terminal::Dashboard;
use polymarket_bot::engine::decision::{Decision, DecisionEngine};
use polymarket_bot::error::BotError;
use polymarket_bot::models::expected_value::EVCalculator;
use polymarket_bot::models::position_sizing::PositionSizer;
use polymarket_bot::models::probability::{ProbabilityModel, Signal};
use polymarket_bot::paper_trading::engine::PaperTradingEngine;
use polymarket_bot::storage::database::Database;
use tempfile::TempDir;

#[test]
fn test_error_display() {
    let err = BotError::ApiError("connection failed".to_string());
    assert_eq!(err.to_string(), "API error: connection failed");
}

#[test]
fn test_error_from_reqwest() {
    let client = reqwest::Client::new();
    let err = client.get("").build().unwrap_err();
    let bot_err: BotError = err.into();
    assert!(matches!(bot_err, BotError::ApiError(_)));
}

#[test]
fn test_config_default_values() {
    let config = Config::default();
    assert_eq!(config.general.initial_capital, 1000.0);
    assert_eq!(config.orderbook.max_spread_pct, 0.04);
    assert_eq!(config.position_sizing.kelly_fraction, 0.125);
}

#[test]
fn test_config_from_toml() {
    let toml = r#"
[general]
initial_capital = 500.0

[orderbook]
max_spread_pct = 0.05
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.general.initial_capital, 500.0);
    assert_eq!(config.orderbook.max_spread_pct, 0.05);
}

#[test]
fn test_market_deserialization() {
    let json = r#"{
        "id": "0x123",
        "question": "Will BTC hit $100k?",
        "outcomePrices": "[0.65, 0.35]",
        "volume": "1000000",
        "endDate": "2026-12-31T00:00:00Z"
    }"#;
    let market: Market = serde_json::from_str(json).unwrap();
    assert_eq!(market.id, "0x123");
    assert_eq!(market.question, "Will BTC hit $100k?");
}

#[test]
fn test_orderbook_level() {
    let level = OrderBookLevel {
        price: 0.65,
        size: 1000.0,
    };
    assert_eq!(level.price, 0.65);
    assert_eq!(level.size, 1000.0);
}

#[tokio::test]
async fn test_gamma_client_new() {
    let client = GammaClient::new("https://gamma-api.polymarket.com");
    assert_eq!(client.base_url, "https://gamma-api.polymarket.com");
}

#[tokio::test]
async fn test_clob_client_new() {
    let client = ClobClient::new("https://clob.polymarket.com");
    assert_eq!(client.base_url, "https://clob.polymarket.com");
}

#[test]
fn test_cli_collect_command() {
    let cli = Cli::try_parse_from(["polymarket", "collect"]).unwrap();
    assert!(matches!(cli.command, Commands::Collect { .. }));
}

#[test]
fn test_cli_trade_dry_run() {
    let cli = Cli::try_parse_from(["polymarket", "trade", "--dry-run"]).unwrap();
    match cli.command {
        Commands::Trade { dry_run, .. } => assert!(dry_run),
        _ => panic!("Expected Trade command"),
    }
}

#[test]
fn test_probability_with_strong_news_signal() {
    let model = ProbabilityModel::new();
    let signals = vec![Signal {
        name: "news".to_string(),
        value: 0.8,
        confidence: 0.9,
    }];
    let prob = model.calculate(0.5, &signals);
    assert!(prob > 0.5, "Strong news signal should increase probability");
}

#[test]
fn test_probability_edge_case_50_50() {
    let model = ProbabilityModel::new();
    let signals = vec![];
    let prob = model.calculate(0.5, &signals);
    assert_eq!(prob, 0.5, "No signals should return prior");
}

#[test]
fn test_probability_clamp_0_1() {
    let model = ProbabilityModel::new();
    let signals = vec![Signal {
        name: "news".to_string(),
        value: 1.0,
        confidence: 1.0,
    }];
    let prob = model.calculate(0.99, &signals);
    assert!(prob <= 1.0, "Probability should not exceed 1.0");
}

#[test]
fn test_ev_positive_when_edge_exists() {
    let calc = EVCalculator::new();
    let ev = calc.calculate(0.6, 0.5, 0.02);
    assert!(ev.ev_net > 0.0, "Positive edge should yield positive EV");
}

#[test]
fn test_ev_negative_when_no_edge() {
    let calc = EVCalculator::new();
    let ev = calc.calculate(0.4, 0.5, 0.02);
    assert!(ev.ev_net < 0.0, "Negative edge should yield negative EV");
}

#[test]
fn test_kelly_sizing_basic() {
    let sizer = PositionSizer::new();
    let size = sizer.calculate_size(0.6, 0.5, 1000.0);
    assert!(size > 0.0, "Kelly should suggest positive size with edge");
}

#[test]
fn test_kelly_respects_max_position() {
    let sizer = PositionSizer::new();
    let size = sizer.calculate_size(0.9, 0.5, 1000.0);
    assert!(size <= 100.0, "Kelly should respect max position limit");
}

#[test]
fn test_obi_bullish() {
    let analyzer = OrderBookAnalyzer::new();
    let snapshot = OrderBookSnapshot {
        bids: vec![OrderBookLevel {
            price: 0.6,
            size: 1000.0,
        }],
        asks: vec![OrderBookLevel {
            price: 0.65,
            size: 500.0,
        }],
    };
    let metrics = analyzer.analyze(&snapshot);
    assert!(
        metrics.order_book_imbalance > 0.0,
        "More bids than asks should be positive OBI"
    );
}

#[test]
fn test_spread_calculation() {
    let analyzer = OrderBookAnalyzer::new();
    let snapshot = OrderBookSnapshot {
        bids: vec![OrderBookLevel {
            price: 0.6,
            size: 1000.0,
        }],
        asks: vec![OrderBookLevel {
            price: 0.65,
            size: 1000.0,
        }],
    };
    let metrics = analyzer.analyze(&snapshot);
    assert!((metrics.spread_pct - 0.08).abs() < 0.001);
}

#[tokio::test]
async fn test_decision_skip_wide_spread() {
    let engine = DecisionEngine::new();
    let decision = engine.evaluate("market1", "Test market", 0.5, 0.6, vec![], 0.0);
    assert!(matches!(decision, Decision::Skip { .. }));
}

#[tokio::test]
async fn test_database_create_tables() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = Database::new(&db_path).await.unwrap();
    assert!(db.path.exists());
}

#[test]
fn test_collector_new() {
    let collector = DataCollector::new("https://gamma-api.polymarket.com".to_string(), 8760.0);
    assert_eq!(collector.gamma_base_url, "https://gamma-api.polymarket.com");
}

#[tokio::test]
async fn test_paper_engine_new() {
    let engine = PaperTradingEngine::new(1000.0);
    assert_eq!(engine.capital, 1000.0);
}

#[test]
fn test_dashboard_new() {
    let dashboard = Dashboard::new();
    assert!(dashboard.is_ok());
}
