use chrono::Utc;
use polymarket_bot::config::Config;
use polymarket_bot::engine::decision::DecisionEngine;
use polymarket_bot::models::probability::Signal;
use polymarket_bot::storage::types::StoredMarket;
use tempfile::TempDir;

#[tokio::test]
async fn test_full_flow_collect_trade() {
    let config = Config::default();
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");

    // Initialize database
    let db = polymarket_bot::storage::database::Database::new(&db_path)
        .await
        .unwrap();

    // Save a market first (required for foreign key constraint)
    let market = StoredMarket {
        id: "market1".to_string(),
        question: "Test market".to_string(),
        yes_price: 0.5,
        no_price: 0.5,
        volume: 10000.0,
        end_date: "2026-12-31".to_string(),
        created_at: Utc::now(),
    };
    db.save_market(&market).await.unwrap();

    // Initialize engines
    let decision_engine = DecisionEngine::new();
    let mut paper_engine = polymarket_bot::paper_trading::engine::PaperTradingEngine::new(
        config.general.initial_capital,
    );

    // Simulate decision
    let signals = vec![Signal {
        name: "news".to_string(),
        value: 0.7,
        confidence: 0.8,
    }];

    let decision = decision_engine.evaluate(
        "market1",
        "Test market",
        0.5,
        0.6,
        signals,
        config.general.initial_capital,
    );

    // Execute trade
    let position = paper_engine.execute_trade(&decision, &db).await;
    assert!(position.is_some(), "Should execute trade");

    // Verify portfolio
    let positions = db.get_open_positions().await.unwrap();
    assert_eq!(positions.len(), 1);

    let summary = paper_engine.get_portfolio_summary(&positions);
    assert!(summary.total_invested > 0.0);
}
