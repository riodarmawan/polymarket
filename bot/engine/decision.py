"""
Decision Engine
===============
Menggabungkan semua komponen untuk mengambil keputusan trading.
"""

from dataclasses import dataclass, field
from typing import Dict, List, Optional
from datetime import datetime
from enum import Enum

from bot.config import BotConfig
from bot.models.probability import BayesianProbabilityModel, Signal, ProbabilityEstimate
from bot.analyzers.orderbook import OrderBookAnalyzer, OrderBookSnapshot, OrderBookMetrics, MarketPhase
from bot.models.expected_value import EVCalculator, EVResult
from bot.models.position_sizing import PositionSizer, PositionSize


class Decision(Enum):
    """Keputusan trading."""
    ENTRY = "entry"
    WAIT = "wait"
    SKIP = "skip"
    EXIT = "exit"


@dataclass
class TradingDecision:
    """Hasil keputusan trading lengkap."""
    # Identifikasi
    market_id: str
    market_question: str
    timestamp: datetime = field(default_factory=datetime.now)
    
    # Keputusan
    decision: Decision = Decision.SKIP
    side: str = "yes"  # "yes" atau "no"
    
    # Probabilitas
    probability: Optional[ProbabilityEstimate] = None
    
    # Order Book
    orderbook_metrics: Optional[OrderBookMetrics] = None
    
    # Expected Value
    ev_result: Optional[EVResult] = None
    
    # Position Sizing
    position_size: Optional[PositionSize] = None
    
    # Summary
    summary: str = ""
    reasons: List[str] = field(default_factory=list)
    warnings: List[str] = field(default_factory=list)
    
    def to_dict(self) -> Dict:
        """Convert ke dictionary."""
        return {
            "market_id": self.market_id,
            "market_question": self.market_question,
            "decision": self.decision.value,
            "side": self.side,
            "timestamp": self.timestamp.isoformat(),
            "summary": self.summary,
            "reasons": self.reasons,
            "warnings": self.warnings,
            "probability": {
                "q_model": self.probability.raw_probability if self.probability else None,
                "q_conservative": self.probability.conservative_probability if self.probability else None,
                "uncertainty": self.probability.uncertainty if self.probability else None,
                "confidence": self.probability.confidence if self.probability else None,
            } if self.probability else None,
            "orderbook": {
                "spread": self.orderbook_metrics.spread if self.orderbook_metrics else None,
                "spread_pct": self.orderbook_metrics.spread_pct if self.orderbook_metrics else None,
                "liquidity_score": self.orderbook_metrics.liquidity_score if self.orderbook_metrics else None,
                "market_phase": self.orderbook_metrics.market_phase.value if self.orderbook_metrics else None,
                "is_tradeable": self.orderbook_metrics.is_tradeable if self.orderbook_metrics else None,
            } if self.orderbook_metrics else None,
            "ev": {
                "ev_net": self.ev_result.ev_net if self.ev_result else None,
                "ev_conservative": self.ev_result.ev_conservative if self.ev_result else None,
                "edge": self.ev_result.edge if self.ev_result else None,
                "recommendation": self.ev_result.recommendation if self.ev_result else None,
            } if self.ev_result else None,
            "position": {
                "position_usd": self.position_size.position_usd if self.position_size else None,
                "position_tokens": self.position_size.position_tokens if self.position_size else None,
                "risk_per_trade": self.position_size.risk_per_trade if self.position_size else None,
                "is_valid": self.position_size.is_valid if self.position_size else None,
            } if self.position_size else None,
        }


class DecisionEngine:
    """
    Decision Engine untuk Polymarket Trading Bot.
    
    Alur:
    1. Ambil data market
    2. Hitung probabilitas model (q)
    3. Analisis order book
    4. Hitung Expected Value
    5. Hitung position size
    6. Ambil keputusan: ENTRY, WAIT, atau SKIP
    """
    
    def __init__(self, config: BotConfig = None):
        self.config = config or BotConfig()
        self.probability_model = BayesianProbabilityModel(self.config.probability)
        self.orderbook_analyzer = OrderBookAnalyzer(self.config.orderbook)
        self.ev_calculator = EVCalculator(self.config.ev)
        self.position_sizer = PositionSizer(self.config.position_sizing)
    
    def evaluate(
        self,
        market_id: str,
        market_question: str,
        market_price: float,
        orderbook: OrderBookSnapshot,
        signals: List[Signal],
        volume_24h: float = 0,
        capital: float = None,
        existing_exposure: float = 0,
        side: str = "yes",
    ) -> TradingDecision:
        """
        Evaluasi market dan buat keputusan trading.
        
        Args:
            market_id: ID market
            market_question: Pertanyaan market
            market_price: Harga market saat ini (YES price)
            orderbook: Snapshot order book
            signals: List sinyal untuk model probabilitas
            volume_24h: Volume 24 jam
            capital: Modal tersedia
            existing_exposure: Total exposure yang sudah ada
            side: "yes" atau "no"
        
        Returns:
            TradingDecision dengan keputusan lengkap
        """
        if capital is None:
            capital = self.config.initial_capital
        
        # Inisialisasi
        decision = TradingDecision(
            market_id=market_id,
            market_question=market_question,
            side=side,
        )
        
        reasons = []
        warnings = []
        
        # Step 1: Hitung probabilitas model
        probability = self.probability_model.estimate(
            prior=market_price,
            signals=signals,
            market_data={"volume": volume_24h, "spread": orderbook.spread or 0}
        )
        decision.probability = probability
        
        reasons.append(f"Model probability: {probability.raw_probability:.2%}")
        reasons.append(f"Conservative probability: {probability.conservative_probability:.2%}")
        reasons.append(f"Uncertainty: {probability.uncertainty:.2%}")
        
        # Step 2: Analisis order book
        orderbook_metrics = self.orderbook_analyzer.analyze(
            orderbook, volume_24h
        )
        decision.orderbook_metrics = orderbook_metrics
        
        reasons.append(f"Market phase: {orderbook_metrics.market_phase.value}")
        reasons.append(f"Liquidity score: {orderbook_metrics.liquidity_score:.2f}")
        
        if not orderbook_metrics.is_tradeable:
            decision.decision = Decision.SKIP
            decision.reasons = reasons
            decision.warnings = ["Market tidak layak ditrade"]
            decision.summary = "SKIP: Market tidak likuid atau spread terlalu lebar"
            return decision
        
        # Step 3: Hitung Expected Value
        p_entry = orderbook_metrics.best_ask
        if side == "no":
            p_entry = 1 - orderbook_metrics.best_bid  # Best ask untuk NO
        
        ev_result = self.ev_calculator.calculate(
            q_model=probability.raw_probability,
            q_conservative=probability.conservative_probability,
            p_entry=p_entry,
            orderbook_metrics=orderbook_metrics,
            market_phase=orderbook_metrics.market_phase,
            side=side,
        )
        decision.ev_result = ev_result
        
        reasons.append(f"Edge: {ev_result.edge:.2%}")
        reasons.append(f"EV net: {ev_result.ev_net:.2%}")
        reasons.append(f"EV conservative: {ev_result.ev_conservative:.2%}")
        reasons.append(f"EV recommendation: {ev_result.recommendation}")
        
        # Step 4: Hitung Position Size
        position_size = self.position_sizer.calculate(
            q_model=probability.raw_probability,
            q_conservative=probability.conservative_probability,
            p_entry=p_entry,
            capital=capital,
            market_phase=orderbook_metrics.market_phase,
            existing_exposure=existing_exposure,
            side=side,
        )
        decision.position_size = position_size
        
        reasons.append(f"Position size: ${position_size.position_usd:.2f}")
        reasons.append(f"Risk per trade: ${position_size.risk_per_trade:.2f}")
        
        if not position_size.is_valid:
            warnings.append(f"Position tidak valid: {position_size.reason}")
        
        # Step 5: Ambil keputusan akhir
        decision.decision = self._make_final_decision(
            ev_result, position_size, orderbook_metrics, probability
        )
        
        # Generate summary
        decision.reasons = reasons
        decision.warnings = warnings
        decision.summary = self._generate_summary(decision)
        
        return decision
    
    def _make_final_decision(
        self,
        ev_result: EVResult,
        position_size: PositionSize,
        orderbook_metrics: OrderBookMetrics,
        probability: ProbabilityEstimate
    ) -> Decision:
        """
        Ambil keputusan akhir berdasarkan semua analisis.
        
        Rules:
        1. Market tidak tradeable → SKIP
        2. EV recommendation ENTRY + position valid → ENTRY
        3. EV recommendation WAIT → WAIT
        4. Sisanya → SKIP
        """
        # Rule 1: Market tidak tradeable
        if not orderbook_metrics.is_tradeable:
            return Decision.SKIP
        
        # Rule 2: Position tidak valid
        if not position_size.is_valid:
            return Decision.WAIT
        
        # Rule 3: Confidence terlalu rendah
        if probability.confidence < self.config.min_confidence:
            return Decision.WAIT
        
        # Rule 4: EV recommendation
        if ev_result.recommendation == "ENTRY":
            return Decision.ENTRY
        
        if ev_result.recommendation == "WAIT":
            return Decision.WAIT
        
        return Decision.SKIP
    
    def _generate_summary(self, decision: TradingDecision) -> str:
        """Generate summary text."""
        if decision.decision == Decision.ENTRY:
            side_text = decision.side.upper()
            ev = decision.ev_result.ev_net if decision.ev_result else 0
            pos = decision.position_size.position_usd if decision.position_size else 0
            return (
                f"ENTRY {side_text}: EV={ev:.2%}, "
                f"Position=${pos:.2f}, "
                f"Market Phase={decision.orderbook_metrics.market_phase.value}"
            )
        elif decision.decision == Decision.WAIT:
            return "WAIT: Kondisi belum optimal, tunggu waktu yang lebih baik"
        else:
            return "SKIP: Market tidak layak ditrade"
    
    def evaluate_exit(
        self,
        market_id: str,
        q_model: float,
        p_current: float,
        p_entry: float,
        side: str = "yes"
    ) -> dict:
        """
        Evaluasi apakah harus exit dari posisi.
        
        Args:
            market_id: ID market
            q_model: Probabilitas model terkini
            p_current: Harga saat ini
            p_entry: Harga saat entry
            side: "yes" atau "no"
        
        Returns:
            Dict dengan exit decision
        """
        return self.ev_calculator.calculate_exit_ev(
            q_model=q_model,
            p_current=p_current,
            p_entry=p_entry,
            side=side
        )
