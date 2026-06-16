use crate::backtesting::types::BacktestResult;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use std::io;

pub fn run_backtest_ui(result: &BacktestResult) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| ui(f, result))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn ui(f: &mut Frame, result: &BacktestResult) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(10),
            Constraint::Min(10),
        ])
        .split(f.area());

    // Header
    let header = Paragraph::new("POLYMARKET BACKTEST RESULTS  [q] quit")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    // Summary
    render_summary(f, result, chunks[1]);

    // Bottom: equity curve + trade log
    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[2]);

    render_equity_curve(f, result, bottom[0]);
    render_trade_log(f, result, bottom[1]);
}

fn render_summary(f: &mut Frame, result: &BacktestResult, area: Rect) {
    let ret = (result.final_capital - result.initial_capital) / result.initial_capital * 100.0;
    let win_rate = if result.total_trades > 0 {
        result.winning_trades as f64 / result.total_trades as f64 * 100.0
    } else {
        0.0
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Capital: ", Style::default().fg(Color::White)),
            Span::styled(
                format!(
                    "${:.2} -> ${:.2}",
                    result.initial_capital, result.final_capital
                ),
                if ret >= 0.0 {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                },
            ),
            Span::raw("  "),
            Span::styled("Return: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:+.2}%", ret),
                if ret >= 0.0 {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("Trades: ", Style::default().fg(Color::White)),
            Span::styled(
                result.total_trades.to_string(),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  "),
            Span::styled("Win: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{} ({:.0}%)", result.winning_trades, win_rate),
                Style::default().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled("Loss: ", Style::default().fg(Color::White)),
            Span::styled(
                result.losing_trades.to_string(),
                Style::default().fg(Color::Red),
            ),
        ]),
        Line::from(vec![
            Span::styled("Fees: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("${:.2}", result.total_fees),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::styled("Max Drawdown: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:.2}%", result.max_drawdown_pct * 100.0),
                Style::default().fg(Color::Red),
            ),
            Span::raw("  "),
            Span::styled("Sharpe: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:.2}", result.sharpe_ratio),
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ];

    let summary =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Summary"));
    f.render_widget(summary, area);
}

fn render_equity_curve(f: &mut Frame, result: &BacktestResult, area: Rect) {
    let height = area.height as usize;
    let width = area.width as usize;

    if result.equity_curve.is_empty() || height < 3 || width < 3 {
        return;
    }

    let values: Vec<f64> = result.equity_curve.iter().map(|p| p.total_equity).collect();
    let min_val = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_val = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = if max_val - min_val > 0.0 {
        max_val - min_val
    } else {
        1.0
    };

    let chart_height = height - 2;
    let chart_width = width - 2;

    let mut lines: Vec<Line> = Vec::new();

    for row in (0..chart_height).rev() {
        let mut spans = Vec::new();
        let threshold = min_val + (range * row as f64 / chart_height as f64);

        for col in 0..chart_width {
            let idx = (col * values.len()) / chart_width;
            let val = values[idx];

            if (val - threshold).abs() < range / chart_height as f64 * 0.5 {
                spans.push(Span::styled("█", Style::default().fg(Color::Green)));
            } else if val > threshold {
                spans.push(Span::styled("█", Style::default().fg(Color::DarkGray)));
            } else {
                spans.push(Span::raw(" "));
            }
        }
        lines.push(Line::from(spans));
    }

    let chart =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Equity Curve"));
    f.render_widget(chart, area);
}

fn render_trade_log(f: &mut Frame, result: &BacktestResult, area: Rect) {
    let items: Vec<ListItem> = result
        .trades
        .iter()
        .rev()
        .take(area.height as usize - 2)
        .map(|t| {
            let (symbol, color) = match t.action.as_str() {
                "BUY_YES" => ("+", Color::Green),
                "BUY_NO" => ("-", Color::Red),
                "SKIP" => (".", Color::DarkGray),
                _ => ("?", Color::White),
            };
            let pnl_str = if t.pnl != 0.0 {
                format!(" {:+.2}", t.pnl)
            } else {
                String::new()
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:>3} ", t.step),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(symbol, Style::default().fg(color)),
                Span::styled(format!(" @ {:.3}", t.price), Style::default().fg(color)),
                Span::styled(pnl_str, Style::default().fg(color)),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Trade Log (newest first)"),
    );
    f.render_widget(list, area);
}
