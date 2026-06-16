use crate::backtesting::types::BacktestResult;

pub fn print_report(result: &BacktestResult) {
    let ret = (result.final_capital - result.initial_capital) / result.initial_capital * 100.0;
    let closed_trades = result.winning_trades + result.losing_trades;
    let win_rate = if closed_trades > 0 {
        result.winning_trades as f64 / closed_trades as f64 * 100.0
    } else {
        0.0
    };

    // Profit factor
    let gross_profit: f64 = result
        .trades
        .iter()
        .filter(|t| t.pnl > 0.0)
        .map(|t| t.pnl)
        .sum();
    let gross_loss: f64 = result
        .trades
        .iter()
        .filter(|t| t.pnl < 0.0)
        .map(|t| t.pnl.abs())
        .sum();
    let profit_factor = if gross_loss > 0.0 {
        gross_profit / gross_loss
    } else if gross_profit > 0.0 {
        999.99
    } else {
        0.0
    };

    // Average trade P&L
    let avg_win = if result.winning_trades > 0 {
        gross_profit / result.winning_trades as f64
    } else {
        0.0
    };
    let avg_loss = if result.losing_trades > 0 {
        gross_loss / result.losing_trades as f64
    } else {
        0.0
    };

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    BACKTEST REPORT                          ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!(
        "║ Initial Capital:  ${:>10.2}                              ║",
        result.initial_capital
    );
    println!(
        "║ Final Capital:    ${:>10.2}                              ║",
        result.final_capital
    );
    println!(
        "║ Total Return:     {:>10.2}%                              ║",
        ret
    );
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!(
        "║ Closed Trades:    {:>10}                              ║",
        closed_trades
    );
    println!(
        "║ Winning Trades:   {:>10} ({:.0}%)                       ║",
        result.winning_trades, win_rate
    );
    println!(
        "║ Losing Trades:    {:>10}                              ║",
        result.losing_trades
    );
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!(
        "║ Avg Win:          ${:>10.4}                              ║",
        avg_win
    );
    println!(
        "║ Avg Loss:         ${:>10.4}                              ║",
        avg_loss
    );
    println!(
        "║ Profit Factor:    {:>10.2}                              ║",
        profit_factor
    );
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!(
        "║ Total Fees:       ${:>10.2}                              ║",
        result.total_fees
    );
    println!(
        "║ Max Drawdown:     {:>10.2}%                              ║",
        result.max_drawdown_pct * 100.0
    );
    println!(
        "║ Sharpe Ratio:     {:>10.2}                              ║",
        result.sharpe_ratio
    );
    println!("╚══════════════════════════════════════════════════════════════╝");

    // Show last 10 trades
    if !result.trades.is_empty() {
        println!("\n  Last Trades:");
        println!(
            "  {:<8} {:<6} {:<10} {:<8} {:<10} {}",
            "Time", "Action", "Market", "Price", "Size", "Reason"
        );
        let start = if result.trades.len() > 10 {
            result.trades.len() - 10
        } else {
            0
        };
        for t in &result.trades[start..] {
            let market_short: String = t.market_id.chars().take(8).collect();
            println!(
                "  {:<8} {:<6} {:<10} {:<8.4} ${:<9.4} {}",
                t.timestamp % 100000,
                t.action,
                market_short,
                t.price,
                t.size_usd,
                t.reason
            );
        }
    }
}
