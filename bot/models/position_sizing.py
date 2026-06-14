"""
Position Sizing - Kelly Criterion
===================================
Menentukan ukuran posisi optimal menggunakan
Kelly Criterion versi konservatif.
"""

from dataclasses import dataclass
from typing import Optional

from bot.analyzers.orderbook import MarketPhase


@dataclass
class PositionSize:
    """Hasil perhitungan ukuran posisi."""
    kelly_fraction: float       # Fraction dari Kelly Criterion
    adjusted_fraction: float    # Setelah disesuaikan (1/8 Kelly, dll)
    position_usd: float         # Ukuran posisi dalam USD
    position_tokens: float      # Ukuran posisi dalam tokens
    risk_per_trade: float       # Risiko per trade (dalam USD)
    risk_pct: float             # Risiko sebagai persentase modal
    is_valid: bool              # Apakah posisi valid
    reason: str                 # Alasan jika tidak valid


class PositionSizer:
    """
    Position sizer menggunakan Kelly Criterion.
    
    Formula Kelly:
    f* = (q * b - (1 - q)) / b
    
    Untuk binary outcome (Polymarket):
    f* = (q - p) / (1 - p)
    
    Dimana:
    - q = probabilitas menurut model
    - p = harga entry
    - f* = fraction modal ideal
    
    Untuk bot, gunakan fractional Kelly (1/8 atau 1/4)
    untuk mengurangi risiko.
    """
    
    def __init__(self, config=None):
        from bot.config import PositionSizingConfig
        self.config = config or PositionSizingConfig()
    
    def calculate(
        self,
        q_model: float,
        q_conservative: float,
        p_entry: float,
        capital: float,
        market_phase: MarketPhase,
        existing_exposure: float = 0,
        side: str = "yes"
    ) -> PositionSize:
        """
        Hitung ukuran posisi optimal.
        
        Args:
            q_model: Probabilitas model
            q_conservative: Probabilitas konservatif
            p_entry: Harga entry
            capital: Total modal
            market_phase: Fase market
            existing_exposure: Total exposure yang sudah ada
            side: "yes" atau "no"
        
        Returns:
            PositionSize dengan detail ukuran posisi
        """
        # Hitung edge
        if side == "yes":
            edge = q_model - p_entry
        else:
            # Untuk NO: probabilitas NO = 1-q, harga NO = 1-p
            edge = (1 - q_model) - (1 - p_entry)
        
        # Kelly fraction (untuk binary)
        if side == "yes":
            kelly_full = self._kelly_binary(q_model, p_entry)
        else:
            kelly_full = self._kelly_binary(1 - q_model, 1 - p_entry)
        
        # Adjust berdasarkan fase market
        kelly_adjusted = self._adjust_for_phase(kelly_full, market_phase)
        
        # Apply fractional Kelly
        adjusted_fraction = kelly_adjusted * self.config.fraction
        
        # Apply max position limit
        max_pct = self.config.max_position_pct
        adjusted_fraction = min(adjusted_fraction, max_pct)
        
        # Hitung position size dalam USD
        position_usd = capital * adjusted_fraction
        
        # Apply min/max limits
        position_usd = max(self.config.min_position_usd, position_usd)
        position_usd = min(self.config.max_position_usd, position_usd)
        
        # Cek max total exposure
        total_exposure = existing_exposure + position_usd
        max_total = capital * self.config.max_total_exposure
        
        if total_exposure > max_total:
            position_usd = max(0, max_total - existing_exposure)
        
        # Hitung tokens
        if p_entry > 0:
            position_tokens = position_usd / p_entry
        else:
            position_tokens = 0
        
        # Hitung risk
        # Risiko = position_usd * (1 - q_conservative) [jika kalah]
        risk_per_trade = position_usd * (1 - q_conservative)
        risk_pct = risk_per_trade / capital if capital > 0 else 0
        
        # Validasi
        is_valid = self._validate(
            position_usd, adjusted_fraction, edge, market_phase, capital
        )
        
        reason = ""
        if not is_valid:
            reason = self._get_invalid_reason(
                position_usd, adjusted_fraction, edge, market_phase
            )
        
        return PositionSize(
            kelly_fraction=round(kelly_full, 4),
            adjusted_fraction=round(adjusted_fraction, 4),
            position_usd=round(position_usd, 2),
            position_tokens=round(position_tokens, 2),
            risk_per_trade=round(risk_per_trade, 2),
            risk_pct=round(risk_pct, 4),
            is_valid=is_valid,
            reason=reason,
        )
    
    def _kelly_binary(self, q: float, p: float) -> float:
        """
        Kelly Criterion untuk binary outcome.
        
        f* = (q - p) / (1 - p)
        
        Args:
            q: Probabilitas menang
            p: Harga entry (odds)
        
        Returns:
            Fraction modal ideal (0-1)
        """
        if p >= 1 or q <= 0:
            return 0
        
        kelly = (q - p) / (1 - p)
        
        # Clamp antara 0 dan 0.5 (tidak pernah all-in)
        return max(0, min(0.5, kelly))
    
    def _adjust_for_phase(self, kelly: float, phase: MarketPhase) -> float:
        """
        Sesuaikan Kelly berdasarkan fase market.
        
        - EARLY: Kurangi 50% (risiko tinggi)
        - ACTIVE: Gunakan 100%
        - NEAR_RESOLUTION: Kurangi 30%
        - ILLIQUID: 0 (jangan trade)
        """
        adjustments = {
            MarketPhase.EARLY: 0.5,
            MarketPhase.ACTIVE: 1.0,
            MarketPhase.NEAR_RESOLUTION: 0.7,
            MarketPhase.ILLIQUID: 0.0,
        }
        
        multiplier = adjustments.get(phase, 0.5)
        return kelly * multiplier
    
    def _validate(
        self,
        position_usd: float,
        fraction: float,
        edge: float,
        phase: MarketPhase,
        capital: float
    ) -> bool:
        """Validasi apakah posisi layak."""
        # Tidak trade di illiquid market
        if phase == MarketPhase.ILLIQUID:
            return False
        
        # Position terlalu kecil
        if position_usd < self.config.min_position_usd:
            return False
        
        # Edge terlalu kecil
        if edge < 0.02:
            return False
        
        # Modal tidak cukup
        if capital < self.config.min_position_usd:
            return False
        
        return True
    
    def _get_invalid_reason(
        self,
        position_usd: float,
        fraction: float,
        edge: float,
        phase: MarketPhase
    ) -> str:
        """Dapatkan alasan mengapa posisi tidak valid."""
        if phase == MarketPhase.ILLIQUID:
            return "Market tidak likuid"
        if position_usd < self.config.min_position_usd:
            return f"Posisi terlalu kecil (${position_usd:.2f})"
        if edge < 0.02:
            return f"Edge terlalu kecil ({edge:.2%})"
        return "Unknown"
    
    def calculate_optimal_entry(
        self,
        q_model: float,
        orderbook_depth: float,
        capital: float
    ) -> dict:
        """
        Hitung harga entry optimal.
        
        Fungsi ini menghitung seberapa besar order yang bisa
        ditempatkan tanpa terlalu banyak mempengaruhi harga.
        """
        # Estimasi price impact
        # Asumsi: setiap 1% dari depth memberikan ~0.5% price impact
        max_position_pct = 0.05  # Maks 5% dari depth
        max_tokens = orderbook_depth * max_position_pct
        
        return {
            "max_tokens_without_impact": round(max_tokens, 2),
            "recommended_tokens": round(max_tokens * 0.5, 2),
            "estimated_price_impact": "0.5-1%",
        }
