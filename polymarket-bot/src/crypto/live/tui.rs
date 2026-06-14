use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use crate::crypto::live::paper_trading::{Stats, Trade, TradeStatus};
use crate::crypto::signals::Direction as TradeDirection;

#[derive(Debug, Clone)]
pub struct MarketInfo {
    pub id: String,
    pub question: String,
    pub yes_price: f64,
    pub no_price: f64,
    pub volume: f64,
}

pub struct TuiRenderer {
    last_signal: Option<String>,
    regime: String,
    current_price: f64,
    price_change_pct: f64,
    uptime_seconds: u64,
    markets: Vec<MarketInfo>,
}

impl TuiRenderer {
    pub fn new() -> Self {
        Self {
            last_signal: None,
            regime: "UNKNOWN".to_string(),
            current_price: 0.0,
            price_change_pct: 0.0,
            uptime_seconds: 0,
            markets: Vec::new(),
        }
    }

    pub fn update_price(&mut self, price: f64, prev_price: f64) {
        self.current_price = price;
        self.price_change_pct = if prev_price > 0.0 {
            ((price - prev_price) / prev_price) * 100.0
        } else {
            0.0
        };
    }

    pub fn update_signal(&mut self, signal: Option<String>) {
        self.last_signal = signal;
    }

    pub fn update_regime(&mut self, regime: String) {
        self.regime = regime;
    }

    pub fn update_uptime(&mut self, seconds: u64) {
        self.uptime_seconds = seconds;
    }

    pub fn update_markets(&mut self, markets: Vec<MarketInfo>) {
        self.markets = markets;
    }

    pub fn render(&self, frame: &mut Frame, stats: &Stats, open_trade: &Option<Trade>, trades: &[Trade]) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),   // Header
                Constraint::Length(3),   // BTC Price
                Constraint::Length(8),   // Market Data
                Constraint::Length(6),   // Signal
                Constraint::Length(5),   // Open Position
                Constraint::Length(6),   // Stats
                Constraint::Min(5),      // Trade History
                Constraint::Length(2),   // Footer
            ])
            .split(frame.area());

        self.render_header(frame, chunks[0], stats);
        self.render_price(frame, chunks[1]);
        self.render_markets(frame, chunks[2]);
        self.render_signal(frame, chunks[3]);
        self.render_open_position(frame, chunks[4], open_trade);
        self.render_stats(frame, chunks[5], stats);
        self.render_trade_history(frame, chunks[6], trades);
        self.render_footer(frame, chunks[7]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect, stats: &Stats) {
        let header = Paragraph::new(Line::from(vec![
            Span::styled("  LIVE TRADING DASHBOARD  ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("                    "),
            Span::styled(
                format!("Capital: ${:.2}", stats.current_capital),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        frame.render_widget(header, area);
    }

    fn render_price(&self, frame: &mut Frame, area: Rect) {
        let arrow = if self.price_change_pct >= 0.0 { "\u{25B2}" } else { "\u{25BC}" };
        let color = if self.price_change_pct >= 0.0 { Color::Green } else { Color::Red };

        let price_line = Line::from(vec![
            Span::raw("  BTC/USDT: "),
            Span::styled(format!("${:.2}", self.current_price), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(format!("{} {:+.2}%", arrow, self.price_change_pct), Style::default().fg(color)),
            Span::raw("    Regime: "),
            Span::styled(&self.regime, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]);

        let price_widget = Paragraph::new(price_line)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
        frame.render_widget(price_widget, area);
    }

    fn render_markets(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = vec![
            Line::from(vec![Span::styled("  POLYMARKET BTC UP/DOWN:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))]),
        ];

        if self.markets.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  No BTC Up/Down markets found", Style::default().fg(Color::DarkGray)),
            ]));
        } else {
            for market in self.markets.iter().take(3) {
                let yes = market.yes_price;
                let no = market.no_price;
                let vol = market.volume;

                let yes_color = if yes >= 0.5 { Color::Green } else { Color::Red };
                let no_color = if no >= 0.5 { Color::Green } else { Color::Red };

                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("[{}]", market.question.chars().take(20).collect::<String>()), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(" YES: "),
                    Span::styled(format!("${:.3}", yes), Style::default().fg(yes_color).add_modifier(Modifier::BOLD)),
                    Span::raw("  NO: "),
                    Span::styled(format!("${:.3}", no), Style::default().fg(no_color).add_modifier(Modifier::BOLD)),
                    Span::raw(format!("  Vol: ${:.0}", vol)),
                ]));
            }
        }

        let market_widget = Paragraph::new(lines)
            .block(Block::default().title("Market").borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        frame.render_widget(market_widget, area);
    }

    fn render_signal(&self, frame: &mut Frame, area: Rect) {
        let signal_text = match &self.last_signal {
            Some(sig) => vec![
                Line::from(vec![
                    Span::styled("  CURRENT SIGNAL:  ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(sig.as_str(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                ]),
            ],
            None => vec![
                Line::from(vec![
                    Span::styled("  Waiting for signal...", Style::default().fg(Color::DarkGray)),
                ]),
            ],
        };

        let signal_widget = Paragraph::new(signal_text)
            .block(Block::default().title("Signal").borders(Borders::ALL).border_style(Style::default().fg(Color::Yellow)));
        frame.render_widget(signal_widget, area);
    }

    fn render_open_position(&self, frame: &mut Frame, area: Rect, trade: &Option<Trade>) {
        let pos_text = match trade {
            Some(t) => {
                let dir_color = match t.direction {
                    TradeDirection::Up => Color::Green,
                    TradeDirection::Down => Color::Red,
                };
                vec![
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("{:?} @ ${:.2}", t.direction, t.entry_price),
                            Style::default().fg(dir_color).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(format!("  Size: ${:.2}", t.size_usd)),
                    ]),
                ]
            }
            None => vec![
                Line::from(vec![
                    Span::styled("  No open position", Style::default().fg(Color::DarkGray)),
                ]),
            ],
        };

        let pos_widget = Paragraph::new(pos_text)
            .block(Block::default().title("Open Position").borders(Borders::ALL).border_style(Style::default().fg(Color::Blue)));
        frame.render_widget(pos_widget, area);
    }

    fn render_stats(&self, frame: &mut Frame, area: Rect, stats: &Stats) {
        let pnl_color = if stats.total_pnl >= 0.0 { Color::Green } else { Color::Red };
        let dd_color = if stats.max_drawdown > 0.25 { Color::Red } else { Color::Yellow };

        let stats_text = vec![
            Line::from(vec![
                Span::raw("  Trades: "),
                Span::styled(stats.total_trades.to_string(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::raw(format!("  Win: {} ({:.1}%)  PnL: ", stats.wins, stats.win_rate * 100.0)),
                Span::styled(format!("${:+.2} ({:+.1}%)", stats.total_pnl, (stats.total_pnl / stats.current_capital) * 100.0), Style::default().fg(pnl_color).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::raw(format!("  Drawdown: ")),
                Span::styled(format!("{:.1}%", stats.max_drawdown * 100.0), Style::default().fg(dd_color)),
                Span::raw(format!("  PF: {:.2}  Avg Win: ${:.2}  Avg Loss: ${:.2}", stats.profit_factor, stats.avg_win, stats.avg_loss)),
            ]),
        ];

        let stats_widget = Paragraph::new(stats_text)
            .block(Block::default().title("Stats").borders(Borders::ALL).border_style(Style::default().fg(Color::Magenta)));
        frame.render_widget(stats_widget, area);
    }

    fn render_trade_history(&self, frame: &mut Frame, area: Rect, trades: &[Trade]) {
        let recent: Vec<&Trade> = trades.iter()
            .rev()
            .take(10)
            .collect();

        let mut lines: Vec<Line> = vec![
            Line::from(vec![Span::styled("  TRADE HISTORY (last 10):", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
        ];

        for trade in recent {
            let (status_icon, pnl_text) = match trade.status {
                TradeStatus::Open => ("\u{25CF}", "...".to_string()),
                TradeStatus::Closed | TradeStatus::Timeout => {
                    match trade.pnl {
                        Some(pnl) => {
                            let icon = if pnl >= 0.0 { "\u{2713}" } else { "\u{2717}" };
                            (icon, format!("${:+.2}", pnl))
                        }
                        None => ("?", "?".to_string()),
                    }
                }
            };

            let dir_color = match trade.direction {
                TradeDirection::Up => Color::Green,
                TradeDirection::Down => Color::Red,
            };

            let exit_str = match trade.exit_price {
                Some(p) => format!("${:.2}", p),
                None => "...".to_string(),
            };

            lines.push(Line::from(vec![
                Span::raw(format!("  {} ", status_icon)),
                Span::styled(
                    format!("{:?} @ ${:.2} -> {}", trade.direction, trade.entry_price, exit_str),
                    Style::default().fg(dir_color),
                ),
                Span::raw(format!("  {}", pnl_text)),
            ]));
        }

        let history_widget = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
        frame.render_widget(history_widget, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let elapsed = self.uptime_seconds;
        let hours = elapsed / 3600;
        let minutes = (elapsed % 3600) / 60;
        let seconds = elapsed % 60;

        let footer = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  Uptime: {}h {}m {}s    Press Ctrl+C to exit", hours, minutes, seconds),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        frame.render_widget(footer, area);
    }
}
