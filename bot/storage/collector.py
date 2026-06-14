"""
Data Collector
==============
Mengambil data dari Gamma API dan CLOB API,
lalu menyimpan ke database.
"""

import asyncio
from datetime import datetime
from typing import Dict, List, Optional

from bot.api.gamma import GammaAPIClient, MarketData
from bot.api.clob import CLOBAPIClient
from bot.storage.database import Database
from bot.analyzers.orderbook import OrderBookAnalyzer, OrderBookSnapshot


class DataCollector:
    """
    Collector untuk mengambil dan menyimpan data Polymarket.
    
    Fungsi:
    1. Fetch market data dari Gamma API
    2. Fetch order book dari CLOB API
    3. Simpan semua ke database
    4. Update berkala (cron)
    """
    
    def __init__(self, db: Database = None):
        self.db = db or Database()
        self.gamma_client = GammaAPIClient()
        self.clob_client = CLOBAPIClient()
        self.orderbook_analyzer = OrderBookAnalyzer()
    
    async def collect_markets(
        self,
        limit: int = 100,
        tags: List[str] = None,
        min_volume: float = 0,
    ) -> List[MarketData]:
        """
        Koleksi data market dari Gamma API.
        
        Args:
            limit: Jumlah market max
            tags: Filter tags
            min_volume: Minimal volume
            
        Returns:
            List MarketData
        """
        print(f"[DataCollector] Fetching {limit} markets...")
        
        markets = await self.gamma_client.fetch_markets(
            limit=limit,
            tags=tags,
            order="volume",
            ascending=False,
        )
        
        # Filter by volume
        if min_volume > 0:
            markets = [m for m in markets if m.volume >= min_volume]
        
        # Simpan ke database
        for market in markets:
            self.db.save_market(market.to_dict())
        
        print(f"[DataCollector] Saved {len(markets)} markets to database")
        return markets
    
    async def collect_orderbook(self, token_id: str) -> Optional[Dict]:
        """
        Koleksi order book dari CLOB API.
        
        Args:
            token_id: Token ID
            
        Returns:
            Dict dengan order book data
        """
        orderbook = await self.clob_client.get_order_book(token_id)
        
        if orderbook:
            # Simpan snapshot
            self.db.save_orderbook_snapshot(orderbook.to_dict())
            
            # Analisis
            snapshot = orderbook.to_snapshot()
            metrics = self.orderbook_analyzer.analyze(snapshot)
            
            return {
                "token_id": token_id,
                "data": orderbook.to_dict(),
                "metrics": {
                    "spread": metrics.spread,
                    "spread_pct": metrics.spread_pct,
                    "bid_depth": metrics.bid_depth,
                    "ask_depth": metrics.ask_depth,
                    "obi": metrics.order_book_imbalance,
                    "liquidity_score": metrics.liquidity_score,
                    "market_phase": metrics.market_phase.value,
                    "is_tradeable": metrics.is_tradeable,
                }
            }
        
        return None
    
    async def collect_price(self, token_id: str) -> Optional[Dict]:
        """
        Koleksi data harga dari CLOB API.
        
        Args:
            token_id: Token ID
            
        Returns:
            Dict dengan price data
        """
        price_data = await self.clob_client.get_market_price(token_id)
        
        if price_data and price_data.get("price"):
            # Simpan ke database
            self.db.save_price(
                token_id=token_id,
                price=price_data["price"],
                midpoint=price_data.get("midpoint"),
                spread=price_data.get("spread", {}).get("spread") if price_data.get("spread") else None,
            )
            
            return price_data
        
        return None
    
    async def collect_trades(
        self,
        market_address: str = None,
        token_id: str = None,
        limit: int = 100,
    ) -> List[Dict]:
        """
        Koleksi trade history.
        
        Args:
            market_address: Market address
            token_id: Token ID
            limit: Jumlah trade max
            
        Returns:
            List trade data
        """
        trades = await self.clob_client.get_trades(
            market=market_address,
            asset_id=token_id,
            limit=limit,
        )
        
        trade_dicts = []
        for trade in trades:
            trade_dict = {
                "id": trade.id,
                "market": trade.market,
                "asset_id": trade.asset_id,
                "side": trade.side,
                "size": trade.size,
                "price": trade.price,
                "timestamp": trade.timestamp,
                "fee_rate_bps": trade.fee_rate_bps,
            }
            trade_dicts.append(trade_dict)
        
        return trade_dicts
    
    async def collect_market_full(self, market_id: str) -> Optional[Dict]:
        """
        Koleksi semua data untuk satu market:
        - Market info
        - Order book (YES dan NO)
        - Price history
        - Trade history
        
        Args:
            market_id: Market ID
            
        Returns:
            Dict lengkap dengan semua data
        """
        # Fetch market info
        market = await self.gamma_client.fetch_market_by_id(market_id)
        
        if not market:
            return None
        
        # Simpan market
        self.db.save_market(market.to_dict())
        
        result = {
            "market": market.to_dict(),
            "orderbook_yes": None,
            "orderbook_no": None,
            "price_yes": None,
            "price_no": None,
        }
        
        # Fetch order book dan price untuk YES
        if market.yes_token_id:
            ob_yes = await self.collect_orderbook(market.yes_token_id)
            price_yes = await self.collect_price(market.yes_token_id)
            result["orderbook_yes"] = ob_yes
            result["price_yes"] = price_yes
        
        # Fetch order book dan price untuk NO
        if market.no_token_id:
            ob_no = await self.collect_orderbook(market.no_token_id)
            price_no = await self.collect_price(market.no_token_id)
            result["orderbook_no"] = ob_no
            result["price_no"] = price_no
        
        return result
    
    async def collect_all_active_markets(self, limit: int = 50) -> List[Dict]:
        """
        Koleksi semua data untuk market aktif.
        
        Args:
            limit: Jumlah market max
            
        Returns:
            List dengan semua data
        """
        # Fetch markets
        markets = await self.collect_markets(limit=limit)
        
        results = []
        for market in markets:
            if market.enable_order_book and market.yes_token_id:
                print(f"[DataCollector] Collecting data for: {market.question[:50]}...")
                data = await self.collect_market_full(market.id)
                if data:
                    results.append(data)
                
                # Rate limiting
                await asyncio.sleep(0.5)
        
        return results
    
    def get_market_analytics(self, market_id: str) -> Dict:
        """
        Analisis data market dari database.
        
        Returns:
            Dict dengan analytics
        """
        market = self.db.get_market(market_id)
        if not market:
            return {"error": "Market not found"}
        
        # Get price history
        clob_token_ids = market.get("clob_token_ids", "[]")
        if isinstance(clob_token_ids, str):
            import json
            clob_token_ids = json.loads(clob_token_ids)
        
        price_history = []
        if clob_token_ids:
            price_history = self.db.get_price_history(clob_token_ids[0], limit=100)
        
        # Get order book history
        ob_history = []
        if clob_token_ids:
            ob_history = self.db.get_orderbook_history(clob_token_ids[0], limit=50)
        
        return {
            "market": market,
            "price_history": price_history,
            "orderbook_history": ob_history,
            "price_count": len(price_history),
            "orderbook_count": len(ob_history),
        }
    
    def get_collection_stats(self) -> Dict:
        """Ambil statistik collection."""
        return self.db.get_stats()
