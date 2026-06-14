"""
Database Storage
================
Menyimpan semua data ke SQLite untuk backtesting dan analisis.
"""

import sqlite3
import json
from datetime import datetime
from typing import Dict, List, Optional
from pathlib import Path


class Database:
    """
    Database manager untuk Polymarket Trading Bot.
    
    Menyimpan:
    - Market data
    - Order book snapshots
    - Price history
    - Trade history
    - Paper trading positions
    - Performance metrics
    """
    
    def __init__(self, db_path: str = None):
        """
        Args:
            db_path: Path ke SQLite database
        """
        if db_path is None:
            db_path = Path(__file__).parent.parent.parent / "data" / "polymarket.db"
        
        self.db_path = Path(db_path)
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        
        self.conn = sqlite3.connect(str(self.db_path))
        self.conn.row_factory = sqlite3.Row
        
        self._create_tables()
    
    def _create_tables(self):
        """Buat semua tabel jika belum ada."""
        cursor = self.conn.cursor()
        
        # Markets table
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS markets (
                id TEXT PRIMARY KEY,
                question TEXT,
                description TEXT,
                outcomes TEXT,
                outcome_prices TEXT,
                volume REAL,
                liquidity REAL,
                end_date TEXT,
                active INTEGER,
                closed INTEGER,
                enable_order_book INTEGER,
                condition_id TEXT,
                question_id TEXT,
                clob_token_ids TEXT,
                market_address TEXT,
                tags TEXT,
                fetched_at TEXT,
                updated_at TEXT
            )
        """)
        
        # Order book snapshots
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS orderbook_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                token_id TEXT,
                bids TEXT,
                asks TEXT,
                hash TEXT,
                spread REAL,
                bid_depth REAL,
                ask_depth REAL,
                mid_price REAL,
                obi REAL,
                timestamp TEXT
            )
        """)
        
        # Price history
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS price_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                token_id TEXT,
                price REAL,
                midpoint REAL,
                spread REAL,
                timestamp TEXT
            )
        """)
        
        # Trade history
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS trade_history (
                id TEXT PRIMARY KEY,
                market TEXT,
                asset_id TEXT,
                side TEXT,
                size REAL,
                price REAL,
                fee_rate_bps INTEGER,
                timestamp TEXT,
                fetched_at TEXT
            )
        """)
        
        # Paper trading positions
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS paper_positions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                market_id TEXT,
                market_question TEXT,
                side TEXT,
                entry_price REAL,
                current_price REAL,
                size_usd REAL,
                size_tokens REAL,
                entry_time TEXT,
                current_time TEXT,
                status TEXT,
                exit_price REAL,
                exit_time TEXT,
                pnl_realized REAL,
                pnl_unrealized REAL
            )
        """)
        
        # Paper trading portfolio
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS paper_portfolio (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                initial_capital REAL,
                current_capital REAL,
                total_invested REAL,
                total_return REAL,
                total_trades INTEGER,
                winning_trades INTEGER,
                losing_trades INTEGER,
                win_rate REAL,
                max_drawdown REAL,
                sharpe_ratio REAL,
                last_updated TEXT
            )
        """)
        
        # Decision history
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS decision_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                market_id TEXT,
                market_question TEXT,
                decision TEXT,
                side TEXT,
                q_model REAL,
                q_conservative REAL,
                p_entry REAL,
                ev_net REAL,
                ev_conservative REAL,
                position_size_usd REAL,
                market_phase TEXT,
                liquidity_score REAL,
                reasons TEXT,
                warnings TEXT,
                timestamp TEXT
            )
        """)
        
        # News/sentiment data
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS news_sentiment (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                market_id TEXT,
                headline TEXT,
                source TEXT,
                sentiment REAL,
                confidence REAL,
                url TEXT,
                published_at TEXT,
                fetched_at TEXT
            )
        """)
        
        self.conn.commit()
    
    # ============ Markets ============
    
    def save_market(self, market_data: Dict):
        """Simpan atau update market data."""
        cursor = self.conn.cursor()
        
        now = datetime.now().isoformat()
        
        cursor.execute("""
            INSERT OR REPLACE INTO markets 
            (id, question, description, outcomes, outcome_prices, volume, liquidity,
             end_date, active, closed, enable_order_book, condition_id, question_id,
             clob_token_ids, market_address, tags, fetched_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            market_data.get("id"),
            market_data.get("question"),
            market_data.get("description"),
            json.dumps(market_data.get("outcomes", [])),
            json.dumps(market_data.get("outcome_prices", [])),
            market_data.get("volume", 0),
            market_data.get("liquidity", 0),
            market_data.get("end_date"),
            1 if market_data.get("active") else 0,
            1 if market_data.get("closed") else 0,
            1 if market_data.get("enable_order_book") else 0,
            market_data.get("condition_id"),
            market_data.get("question_id"),
            json.dumps(market_data.get("clob_token_ids", [])),
            market_data.get("market_address"),
            json.dumps(market_data.get("tags", [])),
            now,
            now,
        ))
        
        self.conn.commit()
    
    def save_markets(self, markets: List[Dict]):
        """Simpan multiple markets."""
        for market in markets:
            self.save_market(market)
    
    def get_market(self, market_id: str) -> Optional[Dict]:
        """Ambil market berdasarkan ID."""
        cursor = self.conn.cursor()
        cursor.execute("SELECT * FROM markets WHERE id = ?", (market_id,))
        row = cursor.fetchone()
        
        if row:
            return dict(row)
        return None
    
    def get_active_markets(self) -> List[Dict]:
        """Ambil semua market aktif."""
        cursor = self.conn.cursor()
        cursor.execute("SELECT * FROM markets WHERE active = 1 AND closed = 0")
        return [dict(row) for row in cursor.fetchall()]
    
    def search_markets(self, query: str) -> List[Dict]:
        """Search market berdasarkan question."""
        cursor = self.conn.cursor()
        cursor.execute(
            "SELECT * FROM markets WHERE question LIKE ?",
            (f"%{query}%",)
        )
        return [dict(row) for row in cursor.fetchall()]
    
    # ============ Order Book ============
    
    def save_orderbook_snapshot(self, snapshot: Dict):
        """Simpan snapshot order book."""
        cursor = self.conn.cursor()
        
        # Hitung metrik
        bids = json.loads(snapshot.get("bids", "[]"))
        asks = json.loads(snapshot.get("asks", "[]"))
        
        bid_depth = sum(float(b.get("size", 0)) for b in bids)
        ask_depth = sum(float(a.get("size", 0)) for a in asks)
        
        if bids and asks:
            best_bid = float(bids[0].get("price", 0))
            best_ask = float(asks[0].get("price", 0))
            mid_price = (best_bid + best_ask) / 2
            spread = best_ask - best_bid
            obi = (bid_depth - ask_depth) / (bid_depth + ask_depth) if (bid_depth + ask_depth) > 0 else 0
        else:
            mid_price = 0
            spread = 0
            obi = 0
        
        cursor.execute("""
            INSERT INTO orderbook_snapshots 
            (token_id, bids, asks, hash, spread, bid_depth, ask_depth, mid_price, obi, timestamp)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            snapshot.get("token_id"),
            json.dumps(bids),
            json.dumps(asks),
            snapshot.get("hash"),
            spread,
            bid_depth,
            ask_depth,
            mid_price,
            obi,
            datetime.now().isoformat(),
        ))
        
        self.conn.commit()
    
    def get_orderbook_history(self, token_id: str, limit: int = 100) -> List[Dict]:
        """Ambil history order book untuk token."""
        cursor = self.conn.cursor()
        cursor.execute(
            "SELECT * FROM orderbook_snapshots WHERE token_id = ? ORDER BY timestamp DESC LIMIT ?",
            (token_id, limit)
        )
        return [dict(row) for row in cursor.fetchall()]
    
    # ============ Price History ============
    
    def save_price(self, token_id: str, price: float, midpoint: float = None, spread: float = None):
        """Simpan data harga."""
        cursor = self.conn.cursor()
        
        cursor.execute("""
            INSERT INTO price_history (token_id, price, midpoint, spread, timestamp)
            VALUES (?, ?, ?, ?, ?)
        """, (token_id, price, midpoint, spread, datetime.now().isoformat()))
        
        self.conn.commit()
    
    def get_price_history(self, token_id: str, limit: int = 1000) -> List[Dict]:
        """Ambil history harga."""
        cursor = self.conn.cursor()
        cursor.execute(
            "SELECT * FROM price_history WHERE token_id = ? ORDER BY timestamp DESC LIMIT ?",
            (token_id, limit)
        )
        return [dict(row) for row in cursor.fetchall()]
    
    # ============ Paper Trading ============
    
    def save_paper_position(self, position: Dict):
        """Simpan paper trading position."""
        cursor = self.conn.cursor()
        
        cursor.execute("""
            INSERT INTO paper_positions 
            (market_id, market_question, side, entry_price, current_price,
             size_usd, size_tokens, entry_time, current_time, status,
             exit_price, exit_time, pnl_realized, pnl_unrealized)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            position.get("market_id"),
            position.get("market_question"),
            position.get("side"),
            position.get("entry_price"),
            position.get("current_price"),
            position.get("size_usd"),
            position.get("size_tokens"),
            position.get("entry_time"),
            position.get("current_time", datetime.now().isoformat()),
            position.get("status", "open"),
            position.get("exit_price"),
            position.get("exit_time"),
            position.get("pnl_realized", 0),
            position.get("pnl_unrealized", 0),
        ))
        
        self.conn.commit()
        return cursor.lastrowid
    
    def update_paper_position(self, position_id: int, updates: Dict):
        """Update paper trading position."""
        cursor = self.conn.cursor()
        
        set_clause = ", ".join(f"{k} = ?" for k in updates.keys())
        values = list(updates.values()) + [position_id]
        
        cursor.execute(
            f"UPDATE paper_positions SET {set_clause} WHERE id = ?",
            values
        )
        
        self.conn.commit()
    
    def get_open_positions(self) -> List[Dict]:
        """Ambil semua posisi terbuka."""
        cursor = self.conn.cursor()
        cursor.execute(
            "SELECT * FROM paper_positions WHERE status = 'open' ORDER BY entry_time DESC"
        )
        return [dict(row) for row in cursor.fetchall()]
    
    def get_all_positions(self, limit: int = 100) -> List[Dict]:
        """Ambil semua posisi."""
        cursor = self.conn.cursor()
        cursor.execute(
            "SELECT * FROM paper_positions ORDER BY entry_time DESC LIMIT ?",
            (limit,)
        )
        return [dict(row) for row in cursor.fetchall()]
    
    # ============ Portfolio ============
    
    def save_portfolio(self, portfolio: Dict):
        """Simpan atau update portfolio."""
        cursor = self.conn.cursor()
        
        # Check if exists
        cursor.execute("SELECT id FROM paper_portfolio LIMIT 1")
        existing = cursor.fetchone()
        
        if existing:
            cursor.execute("""
                UPDATE paper_portfolio SET
                current_capital = ?, total_invested = ?, total_return = ?,
                total_trades = ?, winning_trades = ?, losing_trades = ?,
                win_rate = ?, max_drawdown = ?, sharpe_ratio = ?, last_updated = ?
                WHERE id = ?
            """, (
                portfolio.get("current_capital"),
                portfolio.get("total_invested"),
                portfolio.get("total_return"),
                portfolio.get("total_trades"),
                portfolio.get("winning_trades"),
                portfolio.get("losing_trades"),
                portfolio.get("win_rate"),
                portfolio.get("max_drawdown"),
                portfolio.get("sharpe_ratio"),
                datetime.now().isoformat(),
                existing["id"],
            ))
        else:
            cursor.execute("""
                INSERT INTO paper_portfolio 
                (initial_capital, current_capital, total_invested, total_return,
                 total_trades, winning_trades, losing_trades, win_rate,
                 max_drawdown, sharpe_ratio, last_updated)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """, (
                portfolio.get("initial_capital", 1000),
                portfolio.get("current_capital", 1000),
                portfolio.get("total_invested", 0),
                portfolio.get("total_return", 0),
                portfolio.get("total_trades", 0),
                portfolio.get("winning_trades", 0),
                portfolio.get("losing_trades", 0),
                portfolio.get("win_rate", 0),
                portfolio.get("max_drawdown", 0),
                portfolio.get("sharpe_ratio", 0),
                datetime.now().isoformat(),
            ))
        
        self.conn.commit()
    
    def get_portfolio(self) -> Optional[Dict]:
        """Ambil data portfolio."""
        cursor = self.conn.cursor()
        cursor.execute("SELECT * FROM paper_portfolio LIMIT 1")
        row = cursor.fetchone()
        return dict(row) if row else None
    
    # ============ Decisions ============
    
    def save_decision(self, decision: Dict):
        """Simpan keputusan trading."""
        cursor = self.conn.cursor()
        
        cursor.execute("""
            INSERT INTO decision_history 
            (market_id, market_question, decision, side, q_model, q_conservative,
             p_entry, ev_net, ev_conservative, position_size_usd, market_phase,
             liquidity_score, reasons, warnings, timestamp)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            decision.get("market_id"),
            decision.get("market_question"),
            decision.get("decision"),
            decision.get("side"),
            decision.get("q_model"),
            decision.get("q_conservative"),
            decision.get("p_entry"),
            decision.get("ev_net"),
            decision.get("ev_conservative"),
            decision.get("position_size_usd"),
            decision.get("market_phase"),
            decision.get("liquidity_score"),
            json.dumps(decision.get("reasons", [])),
            json.dumps(decision.get("warnings", [])),
            datetime.now().isoformat(),
        ))
        
        self.conn.commit()
    
    def get_decision_history(self, limit: int = 100) -> List[Dict]:
        """Ambil history keputusan."""
        cursor = self.conn.cursor()
        cursor.execute(
            "SELECT * FROM decision_history ORDER BY timestamp DESC LIMIT ?",
            (limit,)
        )
        return [dict(row) for row in cursor.fetchall()]
    
    # ============ News/Sentiment ============
    
    def save_news_sentiment(self, news: Dict):
        """Simpan data news/sentiment."""
        cursor = self.conn.cursor()
        
        cursor.execute("""
            INSERT INTO news_sentiment 
            (market_id, headline, source, sentiment, confidence, url, published_at, fetched_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            news.get("market_id"),
            news.get("headline"),
            news.get("source"),
            news.get("sentiment"),
            news.get("confidence"),
            news.get("url"),
            news.get("published_at"),
            datetime.now().isoformat(),
        ))
        
        self.conn.commit()
    
    def get_news_for_market(self, market_id: str, limit: int = 50) -> List[Dict]:
        """Ambil news untuk market tertentu."""
        cursor = self.conn.cursor()
        cursor.execute(
            "SELECT * FROM news_sentiment WHERE market_id = ? ORDER BY fetched_at DESC LIMIT ?",
            (market_id, limit)
        )
        return [dict(row) for row in cursor.fetchall()]
    
    # ============ Stats ============
    
    def get_stats(self) -> Dict:
        """Ambil statistik database."""
        cursor = self.conn.cursor()
        
        stats = {}
        
        cursor.execute("SELECT COUNT(*) FROM markets")
        stats["total_markets"] = cursor.fetchone()[0]
        
        cursor.execute("SELECT COUNT(*) FROM markets WHERE active = 1")
        stats["active_markets"] = cursor.fetchone()[0]
        
        cursor.execute("SELECT COUNT(*) FROM orderbook_snapshots")
        stats["orderbook_snapshots"] = cursor.fetchone()[0]
        
        cursor.execute("SELECT COUNT(*) FROM price_history")
        stats["price_records"] = cursor.fetchone()[0]
        
        cursor.execute("SELECT COUNT(*) FROM paper_positions")
        stats["total_positions"] = cursor.fetchone()[0]
        
        cursor.execute("SELECT COUNT(*) FROM paper_positions WHERE status = 'open'")
        stats["open_positions"] = cursor.fetchone()[0]
        
        cursor.execute("SELECT COUNT(*) FROM decision_history")
        stats["total_decisions"] = cursor.fetchone()[0]
        
        return stats
    
    def close(self):
        """Tutup koneksi database."""
        self.conn.close()
