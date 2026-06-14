"""
Paper Trading Engine
====================
Simulasi trading tanpa uang nyata.
Menggunakan data real dari API tapi eksekusi virtual.
"""

import json
from dataclasses import dataclass, field
from typing import Dict, List, Optional
from datetime import datetime
from enum import Enum

from bot.config import BotConfig
from bot.engine.decision import DecisionEngine, TradingDecision, Decision
from bot.models.probability import Signal
from bot.analyzers.orderbook import OrderBookSnapshot, OrderBookLevel
from bot.storage.database import Database


class PositionStatus(Enum):
    """Status posisi."""
    OPEN = "open"
    CLOSED = "closed"
    STOPPED = "stopped"


@dataclass
class PaperPosition:
    """Posisi paper trading."""
    id: int = None
    market_id: str = ""
    market_question: str = ""
    side: str = "yes"
    entry_price: float = 0
    current_price: float = 0
    size_usd: float = 0
    size_tokens: float = 0
    entry_time: str = ""
    status: str = "open"
    exit_price: float = None
    exit_time: str = None
    pnl_realized: float = 0
    pnl_unrealized: float = 0
    
    @property
    def unrealized_pnl_pct(self) -> float:
        """P&L tidak terealisasi dalam persen."""
        if self.entry_price == 0:
            return 0
        return (self.current_price - self.entry_price) / self.entry_price
    
    @property
    def unrealized_pnl_usd(self) -> float:
        """P&L tidak terealisasi dalam USD."""
        return self.size_tokens * (self.current_price - self.entry_price)
    
    def to_dict(self) -> Dict:
        """Convert ke dictionary."""
        return {
            "id": self.id,
            "market_id": self.market_id,
            "market_question": self.market_question,
            "side": self.side,
            "entry_price": self.entry_price,
            "current_price": self.current_price,
            "size_usd": self.size_usd,
            "size_tokens": self.size_tokens,
            "entry_time": self.entry_time,
            "status": self.status,
            "exit_price": self.exit_price,
            "exit_time": self.exit_time,
            "pnl_realized": self.pnl_realized,
            "pnl_unrealized": self.pnl_unrealized,
        }


@dataclass
class PaperPortfolio:
    """Portfolio paper trading."""
    initial_capital: float = 1000.0
    current_capital: float = 1000.0
    total_invested: float = 0
    total_return: float = 0
    total_trades: int = 0
    winning_trades: int = 0
    losing_trades: int = 0
    win_rate: float = 0
    max_drawdown: float = 0
    sharpe_ratio: float = 0
    
    @property
    def total_pnl(self) -> float:
        """Total P&L."""
        return self.current_capital - self.initial_capital
    
    @property
    def total_pnl_pct(self) -> float:
        """Total P&L dalam persen."""
        if self.initial_capital == 0:
            return 0
        return (self.current_capital - self.initial_capital) / self.initial_capital
    
    def update_stats(self):
        """Update statistik portfolio."""
        if self.total_trades > 0:
            self.win_rate = self.winning_trades / self.total_trades
        
        self.total_return = self.total_pnl_pct
    
    def to_dict(self) -> Dict:
        """Convert ke dictionary."""
        return {
            "initial_capital": self.initial_capital,
            "current_capital": self.current_capital,
            "total_invested": self.total_invested,
            "total_return": self.total_return,
            "total_trades": self.total_trades,
            "winning_trades": self.winning_trades,
            "losing_trades": self.losing_trades,
            "win_rate": self.win_rate,
            "max_drawdown": self.max_drawdown,
            "sharpe_ratio": self.sharpe_ratio,
        }


class PaperTradingEngine:
    """
    Engine untuk paper trading.
    
    Alur:
    1. Fetch data real dari API
    2. Jalankan decision engine
    3. Eksekusi virtual (simulasi)
    4. Track posisi dan P&L
    5. Update portfolio
    """
    
    def __init__(self, config: BotConfig = None, db: Database = None):
        self.config = config or BotConfig()
        self.db = db or Database()
        self.decision_engine = DecisionEngine(self.config)
        
        # Load atau inisialisasi portfolio
        self.portfolio = self._load_portfolio()
        
        # Load posisi terbuka
        self.positions = self._load_open_positions()
    
    def _load_portfolio(self) -> PaperPortfolio:
        """Load portfolio dari database."""
        portfolio_data = self.db.get_portfolio()
        
        if portfolio_data:
            return PaperPortfolio(
                initial_capital=portfolio_data.get("initial_capital", 1000),
                current_capital=portfolio_data.get("current_capital", 1000),
                total_invested=portfolio_data.get("total_invested", 0),
                total_return=portfolio_data.get("total_return", 0),
                total_trades=portfolio_data.get("total_trades", 0),
                winning_trades=portfolio_data.get("winning_trades", 0),
                losing_trades=portfolio_data.get("losing_trades", 0),
                win_rate=portfolio_data.get("win_rate", 0),
                max_drawdown=portfolio_data.get("max_drawdown", 0),
                sharpe_ratio=portfolio_data.get("sharpe_ratio", 0),
            )
        
        return PaperPortfolio()
    
    def _load_open_positions(self) -> List[PaperPosition]:
        """Load posisi terbuka dari database."""
        positions_data = self.db.get_open_positions()
        
        positions = []
        for p in positions_data:
            position = PaperPosition(
                id=p.get("id"),
                market_id=p.get("market_id"),
                market_question=p.get("market_question"),
                side=p.get("side"),
                entry_price=p.get("entry_price"),
                current_price=p.get("current_price"),
                size_usd=p.get("size_usd"),
                size_tokens=p.get("size_tokens"),
                entry_time=p.get("entry_time"),
                status=p.get("status"),
            )
            positions.append(position)
        
        return positions
    
    def evaluate_market(
        self,
        market_id: str,
        market_question: str,
        market_price: float,
        orderbook_bids: List[Dict],
        orderbook_asks: List[Dict],
        signals: List[Signal],
        volume_24h: float = 0,
    ) -> TradingDecision:
        """
        Evaluasi market untuk paper trading.
        
        Args:
            market_id: ID market
            market_question: Pertanyaan market
            market_price: Harga market
            orderbook_bids: Bids dari order book
            orderbook_asks: Asks dari order book
            signals: List sinyal
            volume_24h: Volume 24 jam
            
        Returns:
            TradingDecision
        """
        # Buat order book snapshot
        bids = [OrderBookLevel(price=b["price"], size=b["size"]) for b in orderbook_bids]
        asks = [OrderBookLevel(price=a["price"], size=a["size"]) for a in orderbook_asks]
        
        orderbook = OrderBookSnapshot(bids=bids, asks=asks)
        
        # Hitung existing exposure
        existing_exposure = sum(p.size_usd for p in self.positions if p.status == "open")
        
        # Evaluasi
        decision = self.decision_engine.evaluate(
            market_id=market_id,
            market_question=market_question,
            market_price=market_price,
            orderbook=orderbook,
            signals=signals,
            volume_24h=volume_24h,
            capital=self.portfolio.current_capital,
            existing_exposure=existing_exposure,
        )
        
        # Simpan decision ke database
        self.db.save_decision(decision.to_dict())
        
        return decision
    
    def execute_paper_trade(self, decision: TradingDecision) -> Optional[PaperPosition]:
        """
        Eksekusi paper trade berdasarkan keputusan.
        
        Args:
            decision: TradingDecision
            
        Returns:
            PaperPosition atau None jika tidak dieksekusi
        """
        if decision.decision != Decision.ENTRY:
            return None
        
        if not decision.position_size or not decision.position_size.is_valid:
            return None
        
        # Cek apakah sudah ada posisi di market ini
        for pos in self.positions:
            if pos.market_id == decision.market_id and pos.status == "open":
                return None  # Sudah ada posisi
        
        # Cek capital
        if decision.position_size.position_usd > self.portfolio.current_capital:
            return None
        
        # Buat posisi
        position = PaperPosition(
            market_id=decision.market_id,
            market_question=decision.market_question,
            side=decision.side,
            entry_price=decision.orderbook_metrics.best_ask if decision.orderbook_metrics else 0,
            current_price=decision.orderbook_metrics.best_ask if decision.orderbook_metrics else 0,
            size_usd=decision.position_size.position_usd,
            size_tokens=decision.position_size.position_tokens,
            entry_time=datetime.now().isoformat(),
            status="open",
        )
        
        # Simpan ke database
        position_id = self.db.save_paper_position(position.to_dict())
        position.id = position_id
        
        # Update portfolio
        self.portfolio.current_capital -= position.size_usd
        self.portfolio.total_invested += position.size_usd
        self.portfolio.total_trades += 1
        
        self._save_portfolio()
        
        # Tambah ke positions
        self.positions.append(position)
        
        return position
    
    def update_position_price(self, position_id: int, new_price: float):
        """
        Update harga posisi.
        
        Args:
            position_id: ID posisi
            new_price: Harga baru
        """
        for pos in self.positions:
            if pos.id == position_id:
                pos.current_price = new_price
                pos.pnl_unrealized = pos.unrealized_pnl_usd
                
                # Update di database
                self.db.update_paper_position(position_id, {
                    "current_price": new_price,
                    "pnl_unrealized": pos.pnl_unrealized,
                    "current_time": datetime.now().isoformat(),
                })
                break
    
    def close_position(self, position_id: int, exit_price: float = None):
        """
        Tutup posisi.
        
        Args:
            position_id: ID posisi
            exit_price: Harga exit (None = pakai harga market)
        """
        for pos in self.positions:
            if pos.id == position_id and pos.status == "open":
                if exit_price is None:
                    exit_price = pos.current_price
                
                # Hitung P&L
                pnl = pos.size_tokens * (exit_price - pos.entry_price)
                
                # Update posisi
                pos.exit_price = exit_price
                pos.exit_time = datetime.now().isoformat()
                pos.pnl_realized = pnl
                pos.status = "closed"
                
                # Update portfolio
                self.portfolio.current_capital += pos.size_usd + pnl
                if pnl > 0:
                    self.portfolio.winning_trades += 1
                else:
                    self.portfolio.losing_trades += 1
                
                self.portfolio.update_stats()
                self._save_portfolio()
                
                # Update di database
                self.db.update_paper_position(position_id, {
                    "exit_price": exit_price,
                    "exit_time": pos.exit_time,
                    "pnl_realized": pnl,
                    "status": "closed",
                })
                
                # Hapus dari positions
                self.positions = [p for p in self.positions if p.id != position_id]
                
                break
    
    def check_stop_loss(self, stop_loss_pct: float = 0.20):
        """
        Cek dan tutup posisi yang kena stop loss.
        
        Args:
            stop_loss_pct: Stop loss percentage (default 20%)
        """
        for pos in self.positions[:]:  # Copy list untuk avoid modification during iteration
            if pos.status == "open":
                if pos.unrealized_pnl_pct < -stop_loss_pct:
                    self.close_position(pos.id, pos.current_price)
    
    def check_take_profit(self, take_profit_pct: float = 0.30):
        """
        Cek dan tutup posisi yang kena take profit.
        
        Args:
            take_profit_pct: Take profit percentage (default 30%)
        """
        for pos in self.positions[:]:
            if pos.status == "open":
                if pos.unrealized_pnl_pct > take_profit_pct:
                    self.close_position(pos.id, pos.current_price)
    
    def get_portfolio_summary(self) -> Dict:
        """
        Ambil ringkasan portfolio.
        
        Returns:
            Dict dengan ringkasan lengkap
        """
        open_positions = [p for p in self.positions if p.status == "open"]
        
        total_unrealized = sum(p.pnl_unrealized for p in open_positions)
        
        return {
            "portfolio": self.portfolio.to_dict(),
            "open_positions": [p.to_dict() for p in open_positions],
            "open_positions_count": len(open_positions),
            "total_unrealized_pnl": total_unrealized,
            "positions_summary": {
                "total_positions": self.portfolio.total_trades,
                "open": len(open_positions),
                "won": self.portfolio.winning_trades,
                "lost": self.portfolio.losing_trades,
                "win_rate": f"{self.portfolio.win_rate:.2%}",
            },
        }
    
    def get_trade_history(self, limit: int = 50) -> List[Dict]:
        """
        Ambil history trading.
        
        Returns:
            List posisi yang sudah ditutup
        """
        all_positions = self.db.get_all_positions(limit=limit)
        return [p for p in all_positions if p.get("status") == "closed"]
    
    def _save_portfolio(self):
        """Simpan portfolio ke database."""
        self.db.save_portfolio(self.portfolio.to_dict())
    
    def reset(self):
        """Reset paper trading."""
        self.portfolio = PaperPortfolio()
        self.positions = []
        self._save_portfolio()
