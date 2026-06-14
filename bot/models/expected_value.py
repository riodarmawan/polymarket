"""
Expected Value Calculator
=========================
Menghitung expected value dari sebuah posisi trading
dengan mempertimbangkan biaya, risiko, dan probabilitas.
"""

from dataclasses import dataclass
from typing import Optional

from bot.analyzers.orderbook import OrderBookMetrics, MarketPhase


@dataclass
class EVResult:
    """Hasil perhitungan Expected Value."""
    ev_raw: float              # EV tanpa biaya
    ev_net: float              # EV bersih (setelah biaya & risk buffer)
    ev_conservative: float     # EV konservatif (dengan uncertainty)
    edge: float                # Edge = q - p
    cost_total: float          # Total biaya
    is_positive: bool          # Apakah EV positif
    is_above_threshold: bool   # Apakah di atas threshold
    recommendation: str        # Rekomendasi: ENTRY, SKIP, atau WAIT


class EVCalculator:
    """
    Calculator untuk Expected Value trading.
    
    Formula:
    EV_net = q_model - p_entry - cost - risk_buffer
    
    Entry jika:
    EV_net > threshold
    """
    
    def __init__(self, config=None):
        from bot.config import EVConfig
        self.config = config or EVConfig()
    
    def calculate(
        self,
        q_model: float,
        q_conservative: float,
        p_entry: float,
        orderbook_metrics: OrderBookMetrics,
        market_phase: MarketPhase,
        side: str = "yes"
    ) -> EVResult:
        """
        Hitung Expected Value.
        
        Args:
            q_model: Probabilitas menurut model
            q_conservative: Probabilitas konservatif
            p_entry: Harga entry (best ask)
            orderbook_metrics: Metrik order book
            market_phase: Fase market
            side: "yes" atau "no"
        
        Returns:
            EVResult dengan semua kalkulasi
        """
        # Hitung edge
        if side == "yes":
            edge = q_model - p_entry
        else:
            # Untuk NO: probabilitas NO = 1 - q, harga NO = 1 - p
            edge = (1 - q_model) - (1 - p_entry)
        
        # Hitung total cost
        cost_spread = orderbook_metrics.spread_pct * 0.5  # Rata-rata spread cost
        cost_slippage = orderbook_metrics.estimated_slippage
        cost_fee = self.config.cost_per_trade - cost_spread  # Sisa biaya
        
        total_cost = cost_spread + cost_slippage + max(0, cost_fee)
        
        # Risk buffer berdasarkan fase market
        risk_buffer = self._get_risk_buffer(market_phase)
        
        # EV raw (tanpa biaya)
        ev_raw = edge
        
        # EV net (dengan biaya dan risk buffer)
        ev_net = q_model - p_entry - total_cost - risk_buffer
        
        # EV konservatif (menggunakan q_conservative)
        ev_conservative = q_conservative - p_entry - total_cost - risk_buffer
        
        # Threshold berdasarkan fase market
        threshold = self._get_threshold(market_phase)
        
        # Evaluasi
        is_positive = ev_net > 0
        is_above_threshold = ev_conservative > threshold
        
        # Rekomendasi
        recommendation = self._get_recommendation(
            ev_net, ev_conservative, threshold, orderbook_metrics
        )
        
        return EVResult(
            ev_raw=round(ev_raw, 4),
            ev_net=round(ev_net, 4),
            ev_conservative=round(ev_conservative, 4),
            edge=round(edge, 4),
            cost_total=round(total_cost, 4),
            is_positive=is_positive,
            is_above_threshold=is_above_threshold,
            recommendation=recommendation,
        )
    
    def _get_risk_buffer(self, market_phase: MarketPhase) -> float:
        """
        Dapatkan risk buffer berdasarkan fase market.
        
        - EARLY: Buffer lebih besar (risiko lebih tinggi)
        - ACTIVE: Buffer normal
        - NEAR_RESOLUTION: Buffer lebih besar (volatilitas tinggi)
        - ILLIQUID: Jangan trade
        """
        buffers = {
            MarketPhase.EARLY: 0.05,        # 5%
            MarketPhase.ACTIVE: 0.03,       # 3%
            MarketPhase.NEAR_RESOLUTION: 0.05,  # 5%
            MarketPhase.ILLIQUID: 0.10,     # 10%
        }
        return buffers.get(market_phase, self.config.risk_buffer)
    
    def _get_threshold(self, market_phase: MarketPhase) -> float:
        """
        Dapatkan threshold minimal EV berdasarkan fase market.
        
        - EARLY: Threshold lebih tinggi (compensate risk)
        - ACTIVE: Threshold normal
        - NEAR_RESOLUTION: Threshold lebih tinggi
        """
        thresholds = {
            MarketPhase.EARLY: 0.08,        # 8%
            MarketPhase.ACTIVE: 0.05,       # 5%
            MarketPhase.NEAR_RESOLUTION: 0.07,  # 7%
            MarketPhase.ILLIQUID: 999,      # Jangan trade
        }
        return thresholds.get(market_phase, self.config.min_ev_threshold)
    
    def _get_recommendation(
        self,
        ev_net: float,
        ev_conservative: float,
        threshold: float,
        orderbook_metrics: OrderBookMetrics
    ) -> str:
        """
        Berikan rekomendasi berdasarkan kalkulasi.
        
        Returns:
            "ENTRY" - Masuk posisi
            "WAIT" - Tunggu kondisi lebih baik
            "SKIP" - Lewati market ini
        """
        # Jangan trade jika market tidak likuid
        if orderbook_metrics.market_phase == MarketPhase.ILLIQUID:
            return "SKIP"
        
        if not orderbook_metrics.is_tradeable:
            return "SKIP"
        
        # Cek apakah EV konservatif di atas threshold
        if ev_conservative > threshold:
            return "ENTRY"
        
        # Cek apakah EV net positif tapi belum threshold
        if ev_net > 0 and ev_conservative > threshold * 0.5:
            return "WAIT"
        
        # EV negatif
        return "SKIP"
    
    def calculate_exit_ev(
        self,
        q_model: float,
        p_current: float,
        p_entry: float,
        side: str = "yes"
    ) -> dict:
        """
        Hitung EV untuk keputusan exit.
        
        Args:
            q_model: Probabilitas model terkini
            p_current: Harga saat ini
            p_entry: Harga saat entry
            side: "yes" atau "no"
        
        Returns:
            Dict dengan exit metrics
        """
        # Unrealized P&L
        if side == "yes":
            unrealized_pnl = p_current - p_entry
        else:
            unrealized_pnl = (1 - p_current) - (1 - p_entry)
        
        # Current edge
        if side == "yes":
            current_edge = q_model - p_current
        else:
            current_edge = (1 - q_model) - (1 - p_current)
        
        # Decision
        should_exit = False
        reason = ""
        
        # Take profit
        if unrealized_pnl > 0.30:
            should_exit = True
            reason = "TAKE_PROFIT"
        
        # Stop loss
        elif unrealized_pnl < -0.20:
            should_exit = True
            reason = "STOP_LOSS"
        
        # Edge hilang
        elif current_edge < 0.02:
            should_exit = True
            reason = "EDGE_EXHAUSTED"
        
        return {
            "unrealized_pnl": round(unrealized_pnl, 4),
            "current_edge": round(current_edge, 4),
            "should_exit": should_exit,
            "reason": reason,
        }
