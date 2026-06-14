"""
Mock Data Provider
==================
Menyediakan data simulasi realistis untuk testing
ketika API tidak dapat diakses.
"""

import json
import random
from datetime import datetime, timedelta
from typing import Dict, List, Optional


class MockDataProvider:
    """
    Provider data mock untuk simulasi.
    
    Menyediakan:
    - Market data realistis
    - Order book data
    - Price history
    - News/sentiment
    """
    
    # Contoh market realistis
    MOCK_MARKETS = [
        {
            "id": "540817",
            "question": "New Rihanna Album before GTA VI?",
            "description": "Whether Rihanna releases a new studio album before GTA VI is released.",
            "outcomes": '["Yes", "No"]',
            "outcomePrices": '["0.51", "0.49"]',
            "volume": 826502.40,
            "liquidity": 150000,
            "endDate": "2026-07-31T12:00:00Z",
            "active": True,
            "closed": False,
            "enableOrderBook": True,
            "conditionId": "0x1234567890abcdef",
            "questionId": "0xabcdef1234567890",
            "clobTokenIds": '["71321045679252212594626385532706912750332728571942532289631379312455583992563", "71321045679252212594626385532706912750332728571942532289631379312455583992564"]',
            "market": "0x1234567890abcdef1234567890abcdef12345678",
            "tag": '["crypto", "entertainment"]',
        },
        {
            "id": "540843",
            "question": "Will China invade Taiwan before GTA VI?",
            "description": "Whether China launches a military invasion of Taiwan before GTA VI is released.",
            "outcomes": '["Yes", "No"]',
            "outcomePrices": '["0.505", "0.495"]',
            "volume": 1863330.34,
            "liquidity": 250000,
            "endDate": "2026-07-31T12:00:00Z",
            "active": True,
            "closed": False,
            "enableOrderBook": True,
            "conditionId": "0x2345678901bcdef1",
            "questionId": "0xbcdef12345678901",
            "clobTokenIds": '["81321045679252212594626385532706912750332728571942532289631379312455583992563", "81321045679252212594626385532706912750332728571942532289631379312455583992564"]',
            "market": "0x2345678901bcdef1234567890abcdef12345678",
            "tag": '["geopolitics", "war"]',
        },
        {
            "id": "540844",
            "question": "Will Bitcoin hit $1m before GTA VI?",
            "description": "Whether Bitcoin reaches $1,000,000 before GTA VI is released.",
            "outcomes": '["Yes", "No"]',
            "outcomePrices": '["0.4925", "0.5075"]',
            "volume": 4469258.44,
            "liquidity": 500000,
            "endDate": "2026-07-31T12:00:00Z",
            "active": True,
            "closed": False,
            "enableOrderBook": True,
            "conditionId": "0x3456789012cdef12",
            "questionId": "0xcdef123456789012",
            "clobTokenIds": '["91321045679252212594626385532706912750332728571942532289631379312455583992563", "91321045679252212594626385532706912750332728571942532289631379312455583992564"]',
            "market": "0x3456789012cdef1234567890abcdef12345678",
            "tag": '["crypto", "bitcoin"]',
        },
        {
            "id": "540820",
            "question": "Trump out as President before GTA VI?",
            "description": "Whether Donald Trump is no longer President before GTA VI is released.",
            "outcomes": '["Yes", "No"]',
            "outcomePrices": '["0.505", "0.495"]',
            "volume": 665001.89,
            "liquidity": 120000,
            "endDate": "2026-07-31T12:00:00Z",
            "active": True,
            "closed": False,
            "enableOrderBook": True,
            "conditionId": "0x4567890123def123",
            "questionId": "0xdef1234567890123",
            "clobTokenIds": '["10321045679252212594626385532706912750332728571942532289631379312455583992563", "10321045679252212594626385532706912750332728571942532289631379312455583992564"]',
            "market": "0x4567890123def1234567890abcdef12345678",
            "tag": '["politics", "us"]',
        },
        {
            "id": "544092",
            "question": "Will Harvey Weinstein be sentenced to no prison time?",
            "description": "Whether Harvey Weinstein receives no prison time in his sentencing.",
            "outcomes": '["Yes", "No"]',
            "outcomePrices": '["0.821", "0.179"]',
            "volume": 374730.03,
            "liquidity": 80000,
            "endDate": "2025-12-31T12:00:00Z",
            "active": True,
            "closed": False,
            "enableOrderBook": True,
            "conditionId": "0x5678901234ef1234",
            "questionId": "0xef12345678901234",
            "clobTokenIds": '["11321045679252212594626385532706912750332728571942532289631379312455583992563", "11321045679252212594626385532706912750332728571942532289631379312455583992564"]',
            "market": "0x5678901234ef1234567890abcdef12345678",
            "tag": '["legal", "us"]',
        },
    ]
    
    @classmethod
    def get_mock_markets(cls, limit: int = 10) -> List[Dict]:
        """Ambil mock market data."""
        return cls.MOCK_MARKETS[:limit]
    
    @classmethod
    def get_mock_orderbook(cls, yes_price: float = 0.50, spread: float = 0.04) -> Dict:
        """
        Buat mock order book.
        
        Args:
            yes_price: Harga YES
            spread: Spread (default 4%)
        """
        half_spread = spread / 2
        best_bid = yes_price - half_spread
        best_ask = yes_price + half_spread
        
        # Buat beberapa level
        bids = []
        asks = []
        
        for i in range(5):
            bid_price = best_bid - (i * 0.01)
            ask_price = best_ask + (i * 0.01)
            
            bid_size = random.uniform(1000, 10000) * (1 - i * 0.2)
            ask_size = random.uniform(1000, 10000) * (1 - i * 0.2)
            
            bids.append({"price": f"{bid_price:.4f}", "size": f"{bid_size:.0f}"})
            asks.append({"price": f"{ask_price:.4f}", "size": f"{ask_size:.0f}"})
        
        return {
            "bids": bids,
            "asks": asks,
            "hash": f"mock_hash_{random.randint(1000, 9999)}",
        }
    
    @classmethod
    def get_mock_price_history(cls, token_id: str, days: int = 7, points: int = 100) -> List[Dict]:
        """
        Buat mock price history.
        
        Args:
            token_id: Token ID
            days: Jumlah hari
            points: Jumlah data points
        """
        history = []
        base_price = 0.50
        now = datetime.now()
        
        for i in range(points):
            # Random walk
            change = random.uniform(-0.02, 0.02)
            base_price = max(0.05, min(0.95, base_price + change))
            
            timestamp = now - timedelta(hours=(points - i) * (days * 24 / points))
            
            history.append({
                "token_id": token_id,
                "price": round(base_price, 4),
                "midpoint": round(base_price + random.uniform(-0.01, 0.01), 4),
                "spread": round(random.uniform(0.01, 0.05), 4),
                "timestamp": timestamp.isoformat(),
            })
        
        return history
    
    @classmethod
    def get_mock_news(cls, market_id: str) -> List[Dict]:
        """
        Buat mock news data.
        """
        news_templates = [
            {
                "headline": "Market Update: Prices Shift on New Information",
                "source": "Reuters",
                "sentiment": 0.3,
                "confidence": 0.7,
            },
            {
                "headline": "Analysts Weigh In on Current Trends",
                "source": "Bloomberg",
                "sentiment": 0.1,
                "confidence": 0.6,
            },
            {
                "headline": "Breaking: Major Development Reported",
                "source": "CNN",
                "sentiment": 0.5,
                "confidence": 0.8,
            },
            {
                "headline": "Expert Opinion: Uncertainty Remains High",
                "source": "CNBC",
                "sentiment": -0.2,
                "confidence": 0.65,
            },
            {
                "headline": "Latest Polls Show Mixed Results",
                "source": "AP News",
                "sentiment": 0.0,
                "confidence": 0.5,
            },
        ]
        
        news = []
        for template in random.sample(news_templates, min(3, len(news_templates))):
            news.append({
                "market_id": market_id,
                "headline": template["headline"],
                "source": template["source"],
                "sentiment": template["sentiment"] + random.uniform(-0.1, 0.1),
                "confidence": template["confidence"],
                "url": f"https://example.com/news/{random.randint(1000, 9999)}",
                "published_at": datetime.now().isoformat(),
            })
        
        return news
    
    @classmethod
    def get_mock_signals(cls, market_id: str) -> List[Dict]:
        """
        Buat mock signals untuk decision engine.
        """
        news = cls.get_mock_news(market_id)
        
        # Average sentiment dari news
        avg_sentiment = sum(n["sentiment"] for n in news) / len(news) if news else 0
        avg_confidence = sum(n["confidence"] for n in news) / len(news) if news else 0.5
        
        return [
            {"name": "news", "value": avg_sentiment, "confidence": avg_confidence},
            {"name": "polling", "value": random.uniform(-0.3, 0.3), "confidence": 0.6},
            {"name": "price_history", "value": random.uniform(-0.2, 0.2), "confidence": 0.7},
            {"name": "volume", "value": random.uniform(0, 0.5), "confidence": 0.6},
            {"name": "orderbook", "value": random.uniform(-0.2, 0.2), "confidence": 0.65},
        ]
    
    @classmethod
    def simulate_market_movement(cls, current_price: float) -> float:
        """
        Simulasi pergerakan harga.
        
        Args:
            current_price: Harga saat ini
            
        Returns:
            Harga baru
        """
        # Random walk dengan mean reversion
        change = random.gauss(0, 0.01)  # Mean 0, std 0.01
        
        # Mean reversion ke 0.5
        reversion = (0.5 - current_price) * 0.01
        
        new_price = current_price + change + reversion
        
        # Clamp antara 0.05 dan 0.95
        return max(0.05, min(0.95, new_price))
