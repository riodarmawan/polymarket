"""
Gamma API Client
================
Client untuk mengambil data market dari Gamma API.
"""

import json
from dataclasses import dataclass
from typing import Dict, List, Optional
from datetime import datetime


@dataclass
class MarketData:
    """Data market dari Gamma API."""
    id: str
    question: str
    description: str
    outcomes: List[str]
    outcome_prices: List[float]
    volume: float
    liquidity: float
    end_date: Optional[str]
    active: bool
    closed: bool
    enable_order_book: bool
    condition_id: Optional[str]
    question_id: Optional[str]
    clob_token_ids: List[str]
    market_address: Optional[str]
    tags: List[str]
    fetched_at: datetime = None
    
    def __post_init__(self):
        if self.fetched_at is None:
            self.fetched_at = datetime.now()
    
    @property
    def yes_price(self) -> float:
        """Harga YES."""
        return self.outcome_prices[0] if self.outcome_prices else 0
    
    @property
    def no_price(self) -> float:
        """Harga NO."""
        return self.outcome_prices[1] if len(self.outcome_prices) > 1 else 0
    
    @property
    def yes_token_id(self) -> Optional[str]:
        """Token ID untuk YES."""
        return self.clob_token_ids[0] if self.clob_token_ids else None
    
    @property
    def no_token_id(self) -> Optional[str]:
        """Token ID untuk NO."""
        return self.clob_token_ids[1] if len(self.clob_token_ids) > 1 else None
    
    def to_dict(self) -> Dict:
        """Convert ke dictionary."""
        return {
            "id": self.id,
            "question": self.question,
            "description": self.description,
            "outcomes": self.outcomes,
            "outcome_prices": self.outcome_prices,
            "volume": self.volume,
            "liquidity": self.liquidity,
            "end_date": self.end_date,
            "active": self.active,
            "closed": self.closed,
            "enable_order_book": self.enable_order_book,
            "condition_id": self.condition_id,
            "question_id": self.question_id,
            "clob_token_ids": self.clob_token_ids,
            "market_address": self.market_address,
            "tags": self.tags,
            "fetched_at": self.fetched_at.isoformat() if self.fetched_at else None,
        }


class GammaAPIClient:
    """
    Client untuk Gamma API.
    
    API Endpoints:
    - GET /markets - List semua market
    - GET /markets/{id} - Detail market
    - GET /events - List events
    - GET /events/{id} - Detail event
    
    Base URL: https://gamma-api.polymarket.com
    Tidak memerlukan autentikasi.
    """
    
    BASE_URL = "https://gamma-api.polymarket.com"
    
    def __init__(self, use_browser: bool = True):
        """
        Args:
            use_browser: Jika True, gunakan browser fetch (untuk environment yang diblokir)
        """
        self.use_browser = use_browser
        self._cache = {}
    
    async def fetch_markets(
        self,
        limit: int = 100,
        offset: int = 0,
        closed: bool = False,
        active: bool = True,
        tags: List[str] = None,
        order: str = "volume",
        ascending: bool = False,
    ) -> List[MarketData]:
        """
        Fetch list market dari Gamma API.
        
        Args:
            limit: Jumlah market max
            offset: Offset untuk pagination
            closed: Filter market yang sudah tutup
            active: Filter market yang aktif
            tags: Filter berdasarkan tag
            order: Order by (volume, liquidity, end_date)
            ascending: Order ascending
            
        Returns:
            List MarketData
        """
        params = {
            "limit": str(limit),
            "offset": str(offset),
            "closed": str(closed).lower(),
            "order": order,
            "ascending": str(ascending).lower(),
        }
        
        if tags:
            params["tag"] = ",".join(tags)
        
        url = f"{self.BASE_URL}/markets?" + "&".join(f"{k}={v}" for k, v in params.items())
        
        # Fetch data
        data = await self._fetch(url)
        
        # Parse ke MarketData
        markets = []
        for item in data:
            try:
                outcome_prices = json.loads(item.get("outcomePrices", "[]"))
                clob_token_ids = json.loads(item.get("clobTokenIds", "[]"))
                tags_list = json.loads(item.get("tag", "[]")) if item.get("tag") else []
                
                market = MarketData(
                    id=item.get("id", ""),
                    question=item.get("question", ""),
                    description=item.get("description", ""),
                    outcomes=json.loads(item.get("outcomes", "[]")),
                    outcome_prices=outcome_prices,
                    volume=float(item.get("volume", 0)),
                    liquidity=float(item.get("liquidity", 0)),
                    end_date=item.get("endDate"),
                    active=item.get("active", False),
                    closed=item.get("closed", False),
                    enable_order_book=item.get("enableOrderBook", False),
                    condition_id=item.get("conditionId"),
                    question_id=item.get("questionId"),
                    clob_token_ids=clob_token_ids,
                    market_address=item.get("market"),
                    tags=tags_list,
                )
                markets.append(market)
            except Exception as e:
                continue
        
        return markets
    
    async def fetch_market_by_id(self, market_id: str) -> Optional[MarketData]:
        """Fetch detail market berdasarkan ID."""
        url = f"{self.BASE_URL}/markets/{market_id}"
        data = await self._fetch(url)
        
        if not data:
            return None
        
        item = data if isinstance(data, dict) else data[0] if data else None
        if not item:
            return None
        
        return self._parse_market(item)
    
    async def fetch_events(
        self,
        limit: int = 50,
        closed: bool = False,
    ) -> List[Dict]:
        """Fetch list events."""
        url = f"{self.BASE_URL}/events?limit={limit}&closed={str(closed).lower()}"
        return await self._fetch(url)
    
    async def search_markets(self, query: str, limit: int = 20) -> List[MarketData]:
        """Search market berdasarkan query."""
        url = f"{self.BASE_URL}/markets?limit={limit}&closed=false"
        
        # Gamma API tidak punya search endpoint, jadi fetch semua lalu filter
        all_markets = await self.fetch_markets(limit=200)
        
        query_lower = query.lower()
        results = [
            m for m in all_markets
            if query_lower in m.question.lower() or query_lower in m.description.lower()
        ]
        
        return results[:limit]
    
    def _parse_market(self, item: Dict) -> MarketData:
        """Parse item ke MarketData."""
        outcome_prices = json.loads(item.get("outcomePrices", "[]"))
        clob_token_ids = json.loads(item.get("clobTokenIds", "[]"))
        tags_list = json.loads(item.get("tag", "[]")) if item.get("tag") else []
        
        return MarketData(
            id=item.get("id", ""),
            question=item.get("question", ""),
            description=item.get("description", ""),
            outcomes=json.loads(item.get("outcomes", "[]")),
            outcome_prices=outcome_prices,
            volume=float(item.get("volume", 0)),
            liquidity=float(item.get("liquidity", 0)),
            end_date=item.get("endDate"),
            active=item.get("active", False),
            closed=item.get("closed", False),
            enable_order_book=item.get("enableOrderBook", False),
            condition_id=item.get("conditionId"),
            question_id=item.get("questionId"),
            clob_token_ids=clob_token_ids,
            market_address=item.get("market"),
            tags=tags_list,
        )
    
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
            return []
        except Exception as e:
            print(f"Error fetching {url}: {e}")
            return []
