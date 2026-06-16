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

        /// Restrict evaluated market windows to this WIB calendar date (YYYY-MM-DD)
        #[arg(long)]
        date: Option<String>,

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

    /// Report implementation readiness without placing orders
    ProductionReadiness {
        /// Print machine-readable JSON instead of text
        #[arg(long)]
        json: bool,

        /// Exit with failure unless every canary-required gate passes
        #[arg(long)]
        require_canary_ready: bool,
    },

    /// Print the active strategy parameter manifest
    StrategyManifest,

    /// Print forward-test evaluation metrics from the production database
    ForwardReport,

    /// Monitor forward-test quality and fail closed on evidence breaches
    MonitorForward {
        /// Seconds between checks; ignored when max-iterations is 1
        #[arg(long, default_value = "300")]
        interval_secs: u64,

        /// Stop after N checks; zero runs continuously
        #[arg(long, default_value = "0")]
        max_iterations: u64,
    },

    /// Create and verify a POLY_1271 CLOB V2 order locally without submission
    DrySign {
        #[arg(long)]
        token_id: String,
        #[arg(long)]
        price: String,
        #[arg(long)]
        size: String,
    },

    /// Compare authenticated CLOB account state with durable local state
    Reconcile,

    /// Issue one short-lived canary authorization after every gate passes
    AuthorizeCanary {
        #[arg(long)]
        client_key: String,
        #[arg(long, default_value = "0.10")]
        max_usd: f64,
        #[arg(long, default_value = "10")]
        expires_minutes: i64,
        #[arg(long)]
        confirm: String,
    },

    /// Build a redacted operator review packet for a durable canary intent
    CanaryReview {
        #[arg(long)]
        client_key: String,
    },

    /// Submit exactly one previously authorized canary FOK order
    SubmitCanary {
        #[arg(long)]
        authorization_id: String,
        #[arg(long)]
        client_key: String,
        #[arg(long)]
        confirm: String,
    },

    /// Emergency authenticated cancellation of every open CLOB order
    CancelAllLive {
        #[arg(long)]
        confirm: String,
    },

    /// List unresolved production incidents
    Incidents,

    /// Manually resolve an investigated production incident
    ResolveIncident {
        #[arg(long)]
        incident_key: String,
        #[arg(long)]
        confirm: String,
    },

    /// Run the authenticated CLOB user WebSocket lifecycle monitor
    MonitorUserStream {
        /// Stop after N events for a connectivity drill; zero runs continuously
        #[arg(long, default_value = "0")]
        max_events: usize,
    },

    /// Inventory redeemable positions and persist a fail-closed redemption plan
    PlanRedemptions,

    /// Print fail-closed production control-plane status
    OperationalStatus,

    /// Create a consistent SQLite production backup
    Backup {
        #[arg(long)]
        destination: Option<String>,
    },

    /// Verify SQLite database integrity without applying migrations
    VerifyDatabase {
        #[arg(long)]
        path: String,
    },

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
