"""
Polymarket Trading Bot
======================
Sistem trading bot komprehensif untuk Polymarket.

Komponen:
1. Probability Model (Bayesian Updating)
2. Order Book Analyzer (Microstructure)
3. Expected Value Calculator
4. Position Sizing (Kelly Criterion)
5. Decision Engine

Contoh penggunaan:
    from bot import DecisionEngine, BotConfig
    from bot.models.probability import Signal
    from bot.analyzers.orderbook import OrderBookSnapshot, OrderBookLevel
    
    # Inisialisasi
    config = BotConfig()
    engine = DecisionEngine(config)
    
    # Buat signals
    signals = [
        Signal(name="news", value=0.5, confidence=0.8),
        Signal(name="polling", value=0.3, confidence=0.7),
    ]
    
    # Buat order book
    orderbook = OrderBookSnapshot(
        bids=[OrderBookLevel(price=0.48, size=1000)],
        asks=[OrderBookLevel(price=0.52, size=1000)],
    )
    
    # Evaluasi
    decision = engine.evaluate(
        market_id="123",
        market_question="Will X happen?",
        market_price=0.50,
        orderbook=orderbook,
        signals=signals,
    )
    
    print(decision.summary)
"""

from bot.config import BotConfig
from bot.engine.decision import DecisionEngine, TradingDecision, Decision
from bot.models.probability import BayesianProbabilityModel, Signal, ProbabilityEstimate
from bot.analyzers.orderbook import OrderBookAnalyzer, OrderBookSnapshot, OrderBookLevel, MarketPhase
from bot.models.expected_value import EVCalculator, EVResult
from bot.models.position_sizing import PositionSizer, PositionSize

__version__ = "1.0.0"
__author__ = "Rio Trading Bot"

__all__ = [
    # Config
    "BotConfig",
    
    # Decision Engine
    "DecisionEngine",
    "TradingDecision",
    "Decision",
    
    # Probability Model
    "BayesianProbabilityModel",
    "Signal",
    "ProbabilityEstimate",
    
    # Order Book
    "OrderBookAnalyzer",
    "OrderBookSnapshot",
    "OrderBookLevel",
    "MarketPhase",
    
    # Expected Value
    "EVCalculator",
    "EVResult",
    
    # Position Sizing
    "PositionSizer",
    "PositionSize",
]
