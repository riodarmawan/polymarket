use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "polymarket")]
#[command(about = "Polymarket Trading Bot")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Collect market data from APIs
    Collect {
        /// Run as daemon (continuous collection)
        #[arg(long)]
        daemon: bool,

        /// Collection interval in seconds
        #[arg(long, default_value = "300")]
        interval: u64,
    },

    /// Paper trading
    Trade {
        /// Run as daemon (continuous trading)
        #[arg(long)]
        daemon: bool,

        /// Show decisions without execution
        #[arg(long)]
        dry_run: bool,
    },

    /// Backtest strategies with historical data
    Backtest {
        /// Period in days (e.g., 30d)
        #[arg(long, default_value = "30d")]
        period: String,

        /// Strategy to test
        #[arg(long, default_value = "momentum")]
        strategy: String,
    },

    /// Run crypto trading bot
    Crypto {
        /// Run in paper trading mode
        #[arg(long, default_value = "true")]
        paper: bool,

        /// Timeframes to trade (comma-separated)
        #[arg(long, default_value = "5m,15m,1h")]
        timeframes: String,
    },

    /// Backtest crypto trading strategy
    CryptoBacktest {
        /// Period in days
        #[arg(long, default_value = "7")]
        period: u32,

        /// Initial capital in USD
        #[arg(long, default_value = "2.0")]
        capital: f64,

        /// Timeframes to trade (comma-separated)
        #[arg(long, default_value = "15m,1h")]
        timeframes: String,

        /// Source candle interval in minutes (e.g., 15 for 15m candles)
        #[arg(long, default_value = "15")]
        source_interval: u32,
    },

    /// Terminal UI dashboard
    Dashboard {
        /// Refresh interval in seconds
        #[arg(long, default_value = "10")]
        refresh: u64,
    },

    /// Live trading dashboard (paper trading)
    Live {
        /// Initial virtual capital in USD
        #[arg(long, default_value = "2.0")]
        capital: f64,

        /// Maximum order size in USD
        #[arg(long, default_value = "0.50")]
        max_order: f64,
    },

    /// Web trading dashboard
    Web {
        /// Port to serve on
        #[arg(long, default_value = "3001")]
        port: u16,
    },

    /// Validate production prerequisites without placing orders
    ProductionCheck,

    /// Show portfolio
    Portfolio {
        /// Show detailed positions
        #[arg(long)]
        detail: bool,
    },

    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Generate .env file with template
    Init,

    /// Show active configuration
    Show,
}
