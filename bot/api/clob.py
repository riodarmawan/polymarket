"""
CLOB API Client
===============
Client untuk mengambil order book dan data trading dari CLOB API.
"""

import json
from dataclasses import dataclass
from typing import Dict, List, Optional
from datetime import datetime

from bot.analyzers.orderbook import OrderBookSnapshot, OrderBookLevel


@dataclass
class OrderBookData:
    """Data order book dari CLOB API."""
    token_id: str
    bids: List[Dict]  # [{"price": "0.50", "size": "1000"}]
    asks: List[Dict]
    hash: Optional[str] = None
    timestamp: datetime = None
    
    def __post_init__(self):
        if self.timestamp is None:
            self.timestamp = datetime.now()
    
    def to_snapshot(self) -> OrderBookSnapshot:
        """Convert ke OrderBookSnapshot."""
        bids = [
            OrderBookLevel(price=float(b["price"]), size=float(b["size"]))
            for b in self.bids
        ]
        asks = [
            OrderBookLevel(price=float(a["price"]), size=float(a["size"]))
            for a in self.asks
        ]
        
        return OrderBookSnapshot(
            bids=sorted(bids, key=lambda x: x.price, reverse=True),
            asks=sorted(asks, key=lambda x: x.price),
            timestamp=self.timestamp.timestamp() if self.timestamp else 0,
        )
    
    def to_dict(self) -> Dict:
        """Convert ke dictionary."""
        return {
            "token_id": self.token_id,
            "bids": self.bids,
            "asks": self.asks,
            "hash": self.hash,
            "timestamp": self.timestamp.isoformat() if self.timestamp else None,
        }


@dataclass
class PriceData:
    """Data harga dari CLOB API."""
    token_id: str
    price: float
    midpoint: Optional[float] = None
    spread: Optional[float] = None
    last_trade_price: Optional[float] = None
    timestamp: datetime = None
    
    def __post_init__(self):
        if self.timestamp is None:
            self.timestamp = datetime.now()


@dataclass
class TradeData:
    """Data trade history."""
    id: str
    market: str
    asset_id: str
    side: str  # "BUY" atau "SELL"
    size: float
    price: float
    timestamp: str
    fee_rate_bps: int = 0
    
    @property
    def timestamp_dt(self) -> datetime:
        """Convert timestamp ke datetime."""
        try:
            return datetime.fromisoformat(self.timestamp.replace("Z", "+00:00"))
        except:
            return datetime.now()


class CLOBAPIClient:
    """
    Client untuk CLOB API.
    
    API Endpoints:
    - GET /order-book/{token_id} - Order book
    - GET /price/{token_id} - Harga
    - GET /midpoint/{token_id} - Midpoint
    - GET /spread/{token_id} - Spread
    - GET /trades - Trade history
    
    Base URL: https://clob.polymarket.com
    Tidak memerlukan autentikasi untuk read-only.
    """
    
    BASE_URL = "https://clob.polymarket.com"
    
    def __init__(self):
        self._cache = {}
    
    async def get_order_book(self, token_id: str) -> Optional[OrderBookData]:
        """
        Fetch order book untuk token tertentu.
        
        Args:
            token_id: Token ID (YES atau NO)
            
        Returns:
            OrderBookData atau None
        """
        url = f"{self.BASE_URL}/order-book/{token_id}"
        data = await self._fetch(url)
        
        if not data or "bids" not in data:
            return None
        
        return OrderBookData(
            token_id=token_id,
            bids=data.get("bids", []),
            asks=data.get("asks", []),
            hash=data.get("hash"),
        )
    
    async def get_price(self, token_id: str) -> Optional[PriceData]:
        """Fetch harga token."""
        url = f"{self.BASE_URL}/price/{token_id}"
        data = await self._fetch(url)
        
        if not data:
            return None
        
        return PriceData(
            token_id=token_id,
            price=float(data.get("price", 0)),
        )
    
    async def get_midpoint(self, token_id: str) -> Optional[float]:
        """Fetch midpoint harga."""
        url = f"{self.BASE_URL}/midpoint/{token_id}"
        data = await self._fetch(url)
        
        if data and "mid" in data:
            return float(data["mid"])
        return None
    
    async def get_spread(self, token_id: str) -> Optional[Dict]:
        """Fetch spread data."""
        url = f"{self.BASE_URL}/spread/{token_id}"
        data = await self._fetch(url)
        
        return data
    
    async def get_trades(
        self,
        market: str = None,
        asset_id: str = None,
        limit: int = 100,
    ) -> List[TradeData]:
        """
        Fetch trade history.
        
        Args:
            market: Market address
            asset_id: Token ID
            limit: Jumlah trade max
            
        Returns:
            List TradeData
        """
        params = []
        if market:
            params.append(f"market={market}")
        if asset_id:
            params.append(f"asset_id={asset_id}")
        params.append(f"limit={limit}")
        
        url = f"{self.BASE_URL}/trades?" + "&".join(params)
        data = await self._fetch(url)
        
        if not data:
            return []
        
        trades = []
        for item in data:
            try:
                trade = TradeData(
                    id=item.get("id", ""),
                    market=item.get("market", ""),
                    asset_id=item.get("asset_id", ""),
                    side=item.get("side", ""),
                    size=float(item.get("size", 0)),
                    price=float(item.get("price", 0)),
                    timestamp=item.get("timestamp", ""),
                    fee_rate_bps=int(item.get("fee_rate_bps", 0)),
                )
                trades.append(trade)
            except Exception:
                continue
        
        return trades
    
    async def get_market_price(self, token_id: str) -> Dict:
        """
        Fetch semua data harga untuk token.
        
        Returns:
            Dict dengan price, midpoint, spread
        """
        price_task = self.get_price(token_id)
        midpoint_task = self.get_midpoint(token_id)
        spread_task = self.get_spread(token_id)
        
        price_data = await price_task
        midpoint = await midpoint_task
        spread_data = await spread_task
        
        return {
            "token_id": token_id,
            "price": price_data.price if price_data else None,
            "midpoint": midpoint,
            "spread": spread_data,
        }
    
    async def _fetch(self, url: str):
        """Fetch data dari URL menggunakan urllib (built-in)."""
        import urllib.request
        import urllib.error
        import json as json_lib
        
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "PolymarketBot/1.0"})
            with urllib.request.urlopen(req, timeout=30) as response:
                data = json_lib.loads(response.read().decode())
                return data
        except urllib.error.HTTPError as e:
            print(f"HTTP Error fetching {url}: {e.code}")
            return None
        except Exception as e:
            print(f"Error fetching {url}: {e}")
            return None
