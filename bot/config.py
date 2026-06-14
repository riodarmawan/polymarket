"""
Polymarket Trading Bot - Configuration
======================================
Konfigurasi untuk seluruh sistem trading bot.
"""

from dataclasses import dataclass, field
from typing import Dict, List


@dataclass
class OrderBookConfig:
    """Konfigurasi order book analyzer."""
    max_spread: float = 0.04          # Maksimal spread yang diterima (4%)
    min_depth: float = 1000.0         # Minimal depth (dalam token)
    min_volume_24h: float = 10000.0   # Minimal volume 24 jam
    obi_threshold: float = 0.3        # Order Book Imbalance threshold
    slippage_estimate: float = 0.01   # Estimasi slippage default (1%)
    early_spread_threshold: float = 0.08  # Spread > 8% = Early market


@dataclass
class ProbabilityConfig:
    """Konfigurasi model probabilitas Bayesian."""
    prior_weight: float = 0.3         # Bobot probabilitas awal
    news_weight: float = 0.25         # Bobot sinyal berita
    polling_weight: float = 0.15      # Bobot sinyal polling
    price_history_weight: float = 0.1 # Bobot sinyal harga historis
    volume_weight: float = 0.1        # Bobot sinyal volume
    orderbook_weight: float = 0.1     # Bobot sinyal order book
    uncertainty_buffer: float = 0.05  # Buffer ketidakpastian default


@dataclass
class EVConfig:
    """Konfigurasi Expected Value calculator."""
    cost_per_trade: float = 0.02      # Biaya per trade (spread + fee)
    risk_buffer: float = 0.03         # Buffer risiko default
    min_ev_threshold: float = 0.05    # Minimal EV untuk entry (5%)
    min_ev_conservative: float = 0.07 # Minimal EV untuk entry konservatif


@dataclass
class PositionSizingConfig:
    """Konfigurasi position sizing (Kelly Criterion)."""
    fraction: float = 0.125           # 1/8 Kelly (konservatif)
    max_position_pct: float = 0.05    # Maksimal 5% modal per posisi
    min_position_usd: float = 10.0    # Minimal $10 per posisi
    max_position_usd: float = 100.0   # Maksimal $100 per posisi
    max_total_exposure: float = 0.30  # Maksimal 30% modal ter-expose


@dataclass
class MarketPhaseConfig:
    """Konfigurasi deteksi fase market."""
    early_volume_threshold: float = 5000.0     # Volume < ini = Early
    active_volume_threshold: float = 50000.0   # Volume > ini = Active
    near_resolution_days: int = 3               # < 3 hari = Near Resolution
    early_spread_threshold: float = 0.08        # Spread > 8% = Early market


@dataclass
class ExitConfig:
    """Konfigurasi exit strategy."""
    take_profit_pct: float = 0.30      # Take profit di 30%
    stop_loss_pct: float = 0.20        # Stop loss di 20%
    trailing_stop_pct: float = 0.10    # Trailing stop 10%
    edge_exit_threshold: float = 0.02  # Exit jika edge < 2%
    time_exit_hours: int = 72          # Exit setelah 72 jam jika tidak move


@dataclass
class BotConfig:
    """Config utama bot."""
    orderbook: OrderBookConfig = field(default_factory=OrderBookConfig)
    probability: ProbabilityConfig = field(default_factory=ProbabilityConfig)
    ev: EVConfig = field(default_factory=EVConfig)
    position_sizing: PositionSizingConfig = field(default_factory=PositionSizingConfig)
    market_phase: MarketPhaseConfig = field(default_factory=MarketPhaseConfig)
    exit: ExitConfig = field(default_factory=ExitConfig)
    
    # Global settings
    initial_capital: float = 1000.0    # Modal awal
    max_open_positions: int = 10       # Maksimal posisi terbuka
    min_confidence: float = 0.60       # Minimal confidence untuk trading
    enable_trading: bool = False       # False = simulasi, True = real trading
    
    def to_dict(self) -> Dict:
        """Convert config ke dictionary."""
        return {
            "orderbook": {
                "max_spread": self.orderbook.max_spread,
                "min_depth": self.orderbook.min_depth,
                "min_volume_24h": self.orderbook.min_volume_24h,
                "obi_threshold": self.orderbook.obi_threshold,
                "slippage_estimate": self.orderbook.slippage_estimate,
            },
            "probability": {
                "prior_weight": self.probability.prior_weight,
                "news_weight": self.probability.news_weight,
                "polling_weight": self.probability.polling_weight,
                "uncertainty_buffer": self.probability.uncertainty_buffer,
            },
            "ev": {
                "cost_per_trade": self.ev.cost_per_trade,
                "risk_buffer": self.ev.risk_buffer,
                "min_ev_threshold": self.ev.min_ev_threshold,
            },
            "position_sizing": {
                "fraction": self.position_sizing.fraction,
                "max_position_pct": self.position_sizing.max_position_pct,
            },
            "initial_capital": self.initial_capital,
            "enable_trading": self.enable_trading,
        }
