pub mod gamma_client;
pub mod paper_trading;
pub mod tui;

use crate::crypto::binance_ws::{BinanceWsClient, Candle};
use crate::crypto::indicators::Timeframe;
use crate::crypto::signals::SignalEngine;
use anyhow::Result;
use gamma_client::GammaClient;
use paper_trading::{PaperTradingConfig, PaperTradingEngine};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tui::TuiRenderer;

pub struct LiveDashboard {
    config: PaperTradingConfig,
}

impl LiveDashboard {
    pub fn new(capital: f64, max_order: f64) -> Self {
        let config = PaperTradingConfig {
            initial_capital: capital,
            max_order_usd: max_order,
            ..Default::default()
        };
        Self { config }
    }

    pub async fn run(&self) -> Result<()> {
        let ws_client = BinanceWsClient::new(1000);
        let signal_engine = SignalEngine::new();
        let mut paper_engine = PaperTradingEngine::new(self.config.clone());
        let mut tui = TuiRenderer::new();
        let gamma_client = GammaClient::new("http://localhost:3000");

        // Fetch initial market data
        let markets = gamma_client
            .discover_crypto_markets()
            .await
            .unwrap_or_default();
        let market_infos: Vec<tui::MarketInfo> = markets
            .iter()
            .map(|m| tui::MarketInfo {
                id: m.id.clone(),
                question: m.question.clone(),
                yes_price: m.yes_price(),
                no_price: m.no_price(),
                volume: m.volume_usd(),
            })
            .collect();
        tui.update_markets(market_infos);

        // Try to start WebSocket, fallback to mock data
        let ws_ok = ws_client.start().is_ok();
        let mut rx = ws_client.subscribe();

        let mut candle_buffer: Vec<Candle> = Vec::new();
        let mut last_m15_close = 0i64;
        let start_time = Instant::now();

        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;

        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let mut terminal = ratatui::Terminal::new(backend)?;

        // Mock price state for fallback
        let mut mock_price: f64 = 80000.0;
        let mut mock_candle_count = 0u32;
        let mut mock_last_ts = chrono::Utc::now().timestamp_millis();

        let result = self
            .run_loop(
                &mut terminal,
                &mut rx,
                &signal_engine,
                &mut paper_engine,
                &mut tui,
                &mut candle_buffer,
                &mut last_m15_close,
                start_time,
                &gamma_client,
                ws_ok,
                &mut mock_price,
                &mut mock_candle_count,
                &mut mock_last_ts,
            )
            .await;

        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen
        )?;

        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_loop(
        &self,
        terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
        rx: &mut tokio::sync::broadcast::Receiver<Candle>,
        signal_engine: &SignalEngine,
        paper_engine: &mut PaperTradingEngine,
        tui: &mut TuiRenderer,
        candle_buffer: &mut Vec<Candle>,
        last_m15_close: &mut i64,
        start_time: Instant,
        gamma_client: &GammaClient,
        ws_ok: bool,
        mock_price: &mut f64,
        mock_candle_count: &mut u32,
        mock_last_ts: &mut i64,
    ) -> Result<()> {
        let mut prev_price = 0.0;
        let mut market_refresh_counter = 0u32;
        let mut tick_count = 0u32;

        loop {
            // Check for Ctrl+C
            if crossterm::event::poll(Duration::from_millis(100))? {
                if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                    if key.code == crossterm::event::KeyCode::Char('c')
                        && key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL)
                    {
                        break;
                    }
                }
            }

            tick_count += 1;

            // Refresh market data every 60 seconds
            market_refresh_counter += 1;
            if market_refresh_counter >= 600 {
                market_refresh_counter = 0;
                if let Ok(markets) = gamma_client.discover_crypto_markets().await {
                    let market_infos: Vec<tui::MarketInfo> = markets
                        .iter()
                        .map(|m| tui::MarketInfo {
                            id: m.id.clone(),
                            question: m.question.clone(),
                            yes_price: m.yes_price(),
                            no_price: m.no_price(),
                            volume: m.volume_usd(),
                        })
                        .collect();
                    tui.update_markets(market_infos);
                }
            }

            // Get price from WebSocket or generate mock
            let mut got_candle = false;

            if ws_ok {
                // Try WebSocket
                match rx.try_recv() {
                    Ok(candle) => {
                        tui.update_price(candle.close, prev_price);
                        prev_price = candle.close;
                        candle_buffer.push(candle.clone());
                        got_candle = true;

                        if candle_buffer.len() > 100 {
                            candle_buffer.remove(0);
                        }
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {}
                    Err(_) => {}
                }
            }

            // Fallback to mock data if no candle received
            if !got_candle {
                // Generate mock candle every 15 seconds (simulating 1m candle)
                if tick_count % 150 == 0 {
                    *mock_candle_count += 1;
                    let change = (rand::random::<f64>() - 0.5) * 100.0;
                    *mock_price += change;
                    *mock_price = mock_price.max(75000.0).min(85000.0);

                    let now = chrono::Utc::now().timestamp_millis();
                    let candle = Candle {
                        timestamp: *mock_last_ts,
                        open: *mock_price - change,
                        high: *mock_price + rand::random::<f64>() * 50.0,
                        low: *mock_price - rand::random::<f64>() * 50.0,
                        close: *mock_price,
                        volume: rand::random::<f64>() * 100.0,
                    };
                    *mock_last_ts = now;

                    tui.update_price(candle.close, prev_price);
                    prev_price = candle.close;
                    candle_buffer.push(candle);

                    if candle_buffer.len() > 100 {
                        candle_buffer.remove(0);
                    }
                }
            }

            // Check if M15 candle closed (every 15 minutes or 15 mock candles)
            let m15_ts = prev_price as i64 / 100; // Simplified for mock
            if candle_buffer.len() >= 15 && tick_count % 150 == 0 {
                let mut candle_map: HashMap<Timeframe, Vec<Candle>> = HashMap::new();
                candle_map.insert(Timeframe::M15, candle_buffer.clone());

                let signals = signal_engine.generate_signals(&candle_map);

                if let Some(signal) = signals.first() {
                    tui.update_signal(Some(format!(
                        "{} {} (conf: {:.2})",
                        signal.timeframe.as_str(),
                        signal.direction,
                        signal.confidence
                    )));

                    paper_engine.execute_signal(
                        signal.direction.clone(),
                        signal.timeframe,
                        prev_price,
                        chrono::Utc::now().timestamp_millis(),
                    );
                } else {
                    tui.update_signal(None);
                }
            }

            // Check for trade exits
            let now = chrono::Utc::now().timestamp_millis();
            if let Some(_closed) = paper_engine.check_exit(prev_price, now) {
                // Trade closed
            }

            // Update uptime and render
            tui.update_uptime(start_time.elapsed().as_secs());

            let stats = paper_engine.stats();
            let open_trade = paper_engine.open_trade.clone();
            let trades = paper_engine.trades.clone();

            terminal.draw(|frame| {
                tui.render(frame, &stats, &open_trade, &trades);
            })?;

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Ok(())
    }
}
