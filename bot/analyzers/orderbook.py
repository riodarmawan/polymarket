"""
Order Book Analyzer
===================
Menganalisis microstructure order book untuk menentukan
kondisi market dan harga entry yang optimal.
"""

from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple
from enum import Enum


class MarketPhase(Enum):
    """Fase market berdasarkan kondisi."""
    EARLY = "early"              # Market baru, spread lebar, volume rendah
    ACTIVE = "active"            # Market aktif, spread normal
    NEAR_RESOLUTION = "near_resolution"  # Mendekati resolusi
    ILLIQUID = "illiquid"        # Tidak likuid, hindari


@dataclass
class OrderBookLevel:
    """Level di order book."""
    price: float
    size: float


@dataclass
class OrderBookSnapshot:
    """Snapshot order book pada waktu tertentu."""
    bids: List[OrderBookLevel] = field(default_factory=list)
    asks: List[OrderBookLevel] = field(default_factory=list)
    timestamp: float = 0
    
    @property
    def best_bid(self) -> Optional[float]:
        """Harga beli terbaik (tertinggi)."""
        return self.bids[0].price if self.bids else None
    
    @property
    def best_ask(self) -> Optional[float]:
        """Harga jual terbaik (terendah)."""
        return self.asks[0].price if self.asks else None
    
    @property
    def spread(self) -> Optional[float]:
        """Spread antara best bid dan best ask."""
        if self.best_bid and self.best_ask:
            return self.best_ask - self.best_bid
        return None
    
    @property
    def mid_price(self) -> Optional[float]:
        """Harga tengah (midpoint)."""
        if self.best_bid and self.best_ask:
            return (self.best_bid + self.best_ask) / 2
        return None
    
    @property
    def bid_depth(self) -> float:
        """Total depth bid."""
        return sum(level.size for level in self.bids)
    
    @property
    def ask_depth(self) -> float:
        """Total depth ask."""
        return sum(level.size for level in self.asks)
    
    @property
    def total_depth(self) -> float:
        """Total depth (bid + ask)."""
        return self.bid_depth + self.ask_depth


@dataclass
class OrderBookMetrics:
    """Metrik hasil analisis order book."""
    best_bid: float
    best_ask: float
    mid_price: float
    spread: float
    spread_pct: float           # Spread sebagai persentase
    bid_depth: float
    ask_depth: float
    total_depth: float
    order_book_imbalance: float  # OBI: -1 sampai 1
    depth_ratio: float           # bid/ask depth ratio
    estimated_slippage: float    # Estimasi slippage untuk order tertentu
    market_phase: MarketPhase
    liquidity_score: float       # Skor likuiditas 0-1
    is_tradeable: bool           # Apakah layak ditrade
    
    
class OrderBookAnalyzer:
    """
    Analyzer untuk order book Polymarket.
    
    Fungsi:
    1. Hitung metrik order book (spread, depth, OBI)
    2. Deteksi fase market
    3. Estimasi slippage
    4. Tentukan apakah market layak ditrade
    """
    
    def __init__(self, config=None):
        from bot.config import OrderBookConfig
        self.config = config or OrderBookConfig()
    
    def analyze(
        self,
        orderbook: OrderBookSnapshot,
        volume_24h: float = 0,
        price_history: List[float] = None
    ) -> OrderBookMetrics:
        """
        Analisis order book lengkap.
        
        Args:
            orderbook: Snapshot order book
            volume_24h: Volume 24 jam terakhir
            price_history: Histori harga (untuk volatility)
        
        Returns:
            OrderBookMetrics dengan semua metrik
        """
        if orderbook.best_bid is None or orderbook.best_ask is None:
            return self._empty_metrics()
        
        spread = orderbook.spread
        mid = orderbook.mid_price
        
        # Hitung spread percentage
        spread_pct = spread / mid if mid > 0 else 1.0
        
        # Hitung Order Book Imbalance (OBI)
        obi = self._calculate_obi(orderbook)
        
        # Hitung depth ratio
        depth_ratio = (
            orderbook.bid_depth / orderbook.ask_depth 
            if orderbook.ask_depth > 0 
            else 1.0
        )
        
        # Estimasi slippage
        slippage = self._estimate_slippage(orderbook, side="buy")
        
        # Deteksi fase market
        market_phase = self._detect_phase(
            spread_pct, orderbook.total_depth, volume_24h
        )
        
        # Hitung liquidity score
        liquidity_score = self._calculate_liquidity_score(
            orderbook, volume_24h, spread_pct
        )
        
        # Tentukan apakah layak trade
        is_tradeable = self._is_tradeable(
            spread_pct, orderbook.total_depth, volume_24h, liquidity_score
        )
        
        return OrderBookMetrics(
            best_bid=orderbook.best_bid,
            best_ask=orderbook.best_ask,
            mid_price=mid,
            spread=spread,
            spread_pct=round(spread_pct, 4),
            bid_depth=orderbook.bid_depth,
            ask_depth=orderbook.ask_depth,
            total_depth=orderbook.total_depth,
            order_book_imbalance=round(obi, 4),
            depth_ratio=round(depth_ratio, 4),
            estimated_slippage=round(slippage, 4),
            market_phase=market_phase,
            liquidity_score=round(liquidity_score, 4),
            is_tradeable=is_tradeable,
        )
    
    def _calculate_obi(self, orderbook: OrderBookSnapshot) -> float:
        """
        Hitung Order Book Imbalance (OBI).
        
        OBI = (bid_depth - ask_depth) / (bid_depth + ask_depth)
        
        Range: -1 sampai 1
        - Positif = tekanan beli (bullish untuk YES)
        - Negatif = tekanan jual (bearish untuk YES)
        """
        total = orderbook.total_depth
        if total == 0:
            return 0.0
        
        return (orderbook.bid_depth - orderbook.ask_depth) / total
    
    def _estimate_slippage(
        self, 
        orderbook: OrderBookSnapshot, 
        side: str = "buy",
        order_size: float = 100
    ) -> float:
        """
        Estimasi slippage untuk order tertentu.
        
        Args:
            orderbook: Order book snapshot
            side: "buy" atau "sell"
            order_size: Ukuran order (dalam token)
        
        Returns:
            Estimasi slippage sebagai decimal
        """
        levels = orderbook.asks if side == "buy" else orderbook.bids
        
        if not levels:
            return self.config.slippage_estimate
        
        # Simulasi market order
        remaining = orderbook.ask_depth if side == "buy" else orderbook.bid_depth
        if remaining == 0:
            return self.config.slippage_estimate
        
        # Hitung weighted average price
        total_cost = 0
        tokens_filled = 0
        
        for level in levels:
            if tokens_filled >= order_size:
                break
            fill_amount = min(level.size, order_size - tokens_filled)
            total_cost += fill_amount * level.price
            tokens_filled += fill_amount
        
        if tokens_filled == 0:
            return self.config.slippage_estimate
        
        avg_price = total_cost / tokens_filled
        mid = orderbook.mid_price
        
        if mid and mid > 0:
            slippage = abs(avg_price - mid) / mid
            return min(slippage, 0.10)  # Cap at 10%
        
        return self.config.slippage_estimate
    
    def _detect_phase(
        self,
        spread_pct: float,
        total_depth: float,
        volume_24h: float
    ) -> MarketPhase:
        """
        Deteksi fase market.
        
        Phases:
        - EARLY: Spread lebar, volume rendah
        - ACTIVE: Spread normal, volume cukup
        - NEAR_RESOLUTION: Volume sangat tinggi (biasanya menjelang resolusi)
        - ILLIQUID: Depth sangat rendah
        """
        if total_depth < 500:
            return MarketPhase.ILLIQUID
        
        if spread_pct > self.config.early_spread_threshold:
            if volume_24h < 5000:
                return MarketPhase.EARLY
        
        if volume_24h > 100000:
            return MarketPhase.NEAR_RESOLUTION
        
        if spread_pct < 0.05 and total_depth > 5000:
            return MarketPhase.ACTIVE
        
        return MarketPhase.ACTIVE
    
    def _calculate_liquidity_score(
        self,
        orderbook: OrderBookSnapshot,
        volume_24h: float,
        spread_pct: float
    ) -> float:
        """
        Hitung skor likuiditas 0-1.
        
        Komponen:
        - Depth score (40%)
        - Volume score (30%)
        - Spread score (30%)
        """
        # Depth score (semakin banyak semakin bagus)
        depth_score = min(1.0, orderbook.total_depth / 10000)
        
        # Volume score
        volume_score = min(1.0, volume_24h / 50000)
        
        # Spread score (semakin kecil semakin bagus)
        spread_score = max(0, 1.0 - (spread_pct / 0.10))
        
        # Weighted average
        score = (
            0.4 * depth_score +
            0.3 * volume_score +
            0.3 * spread_score
        )
        
        return max(0, min(1, score))
    
    def _is_tradeable(
        self,
        spread_pct: float,
        total_depth: float,
        volume_24h: float,
        liquidity_score: float
    ) -> bool:
        """
        Tentukan apakah market layak ditrade.
        
        Kriteria:
        - Spread < threshold
        - Depth > minimum
        - Volume > minimum
        - Liquidity score cukup
        """
        if spread_pct > self.config.max_spread:
            return False
        
        if total_depth < self.config.min_depth:
            return False
        
        if volume_24h < self.config.min_volume_24h:
            return False
        
        if liquidity_score < 0.3:
            return False
        
        return True
    
    def _empty_metrics(self) -> OrderBookMetrics:
        """Return empty metrics jika order book kosong."""
        return OrderBookMetrics(
            best_bid=0,
            best_ask=0,
            mid_price=0,
            spread=0,
            spread_pct=1.0,
            bid_depth=0,
            ask_depth=0,
            total_depth=0,
            order_book_imbalance=0,
            depth_ratio=0,
            estimated_slippage=0.1,
            market_phase=MarketPhase.ILLIQUID,
            liquidity_score=0,
            is_tradeable=False,
        )
    
    def get_entry_price(
        self, 
        orderbook: OrderBookSnapshot, 
        side: str = "buy"
    ) -> Optional[float]:
        """
        Dapatkan harga entry efektif.
        
        Untuk beli YES: best ask
        Untuk beli NO: best ask NO
        """
        if side == "buy":
            return orderbook.best_ask
        else:
            return orderbook.best_bid
