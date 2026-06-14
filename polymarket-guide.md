# Polymarket Comprehensive Guide

> Panduan lengkap Polymarket - Didokumentasikan dari dokumentasi resmi dan sumber terpercaya
> Terakhir diperbarui: Juni 2026

## Table of Contents
1. [Apa itu Polymarket](#apa-itu-polymarket)
2. [Konsep Inti](#konsep-inti)
3. [Jenis Market](#jenis-market)
4. [Cara Kerja Trading](#cara-kerja-trading)
5. [Jenis Order](#jenis-order)
6. [Struktur Biaya](#struktur-biaya)
7. [Proses Settlement](#proses-settlement)
8. [Infrastruktur Teknis](#infrastruktur-teknis)
9. [Dokumentasi API](#dokumentasi-api)
10. [Setup Wallet](#setup-wallet)
11. [Peringatan Risiko](#peringatan-risiko)
12. [Strategi Trading](#strategi-trading)
13. [Sumber Daya](#sumber-daya)

---

## Apa itu Polymarket

Polymarket adalah platform prediksi terdesentralisasi terbesar di dunia di mana pengguna bertransaksi pada hasil acara dunia nyata. Alih-alih bertaruh melawan rumah, Anda berbagi saham dengan pengguna lain di pasar peer-to-peer yang terbuka.

**Statistik Utama:**
- **Volume Trading Bulanan:** $7B+ (Miliar USD)
- **Jumlah Market:** 500+ market aktif
- **Blockchain:** Polygon (Layer-2 Ethereum)
- **Mata Uang:** USDC (USD Coin, stablecoin 1:1 dengan USD)
- **Model:** Non-custodial (Anda kendalikan dana Anda)

**Prinsip Inti:** Harga mencerminkan keyakinan kolektif pasar terhadap probabilitas terjadinya suatu peristiwa.

**Sumber:**
- Polymarket 101 (docs.polymarket.com)
- Polymarket Basics (pred101.com)
- How Polymarket Works (learnpolymarket.com)

---

## Konsep Inti

### Harga = Probabilitas

Setiap saham di Polymarket dihargai antara $0.00 dan $1.00. Harga merepresentasikan keyakinan pasar terhadap probabilitas outcome tersebut terjadi.

**Contoh:**
- Harga saham $0.65 = probabilitas implisit 65%
- Harga saham $0.20 = probabilitas implisit 20%
- Harga saham $0.85 = probabilitas implisit 85%

### Hubungan Yes/No

- **YES price + NO price ≈ $1.00** (dikurangi spread kecil)
- Jika YES = $0.65, NO seharusnya sekitar $0.35
- Hubungan ini menciptakan peluang arbitrage dan menjaga harga tetap jujur

### Model Self-Custody

Polymarket beroperasi dengan model non-custodial:

| Aspek | Penjelasan |
|-------|------------|
| **Kontrol Dana** | Aset disimpan di wallet Anda, diamankan oleh private key |
| **Smart Contract** | Transaksi dieksekusi otomatis melalui smart contract yang telah diaudit |
| **Tanpa Perantara** | Polymarket tidak pernah mengambil kepemilikan dana Anda |
| **Transparansi Penuh** | Semua transaksi dan posisi tercatat di blockchain |
| **Eksekusi Trustless** | Settlement terjadi otomatis berdasarkan resolusi market |

### Model Hybrid: AMM + CLOB

Polymarket menggunakan struktur pasar hybrid:

| Model | Keterangan | Cocok untuk |
|-------|------------|-------------|
| **AMM (Automated Market Maker)** | Pool likuiditas algoritmik, harga berdasarkan supply/demand | Market kecil/baru |
| **CLOB (Central Limit Order Book)** | Order book tradisional seperti bursa saham | Market besar/likuid |

**Catatan Penting:**
- AMM: Membeli posisi besar akan mendorong harga melawan Anda (price impact)
- CLOB: Memungkinkan limit order di harga yang Anda inginkan

---

## Jenis Market

| Jenis | Deskripsi | Contoh |
|-------|-----------|--------|
| **Binary** | Outcome Yes/No | "Will BTC reach $100K in 2026?" |
| **Multiple Choice** | Beberapa outcome mungkin | "Who will win the 2026 Super Bowl?" |
| **Numerical** | Prediksi rentang nilai | "What will the Fed rate be in 2026?" |
| **Sprint** | Settlement cepat jangka pendek (5-15 menit) | "Will BTC go up in the next 5 minutes?" |

### Single-Market vs Multi-Market Events

| Tipe | Contoh |
|------|--------|
| Single-market event | "Will Bitcoin reach $100k?" → 1 market (Yes/No) |
| Multi-market event | "Where will Barron Trump attend College?" → Market untuk Georgetown, NYU, UPenn, Harvard, Other |

### Outcomes dan Prices

Setiap market memiliki array `outcomes` dan `outcomePrices` yang mapping 1:1:

```json
{
  "outcomes": "[\"Yes\", \"No\"]",
  "outcomePrices": "[\"0.20\", \"0.80\"]"
}
// Index 0: "Yes" → 0.20 (20% probabilitas)
// Index 1: "No" → 0.80 (80% probabilitas)
```

**Catatan:** Market dapat diperdagangkan via CLOB jika `enableOrderBook` bernilai `true`.

---

## Cara Kerja Trading

### Langkah-langkah Trading

1. **Buat akun** → polymarket.com, signup dengan email (custodial) atau connect MetaMask
2. **Verifikasi KYC** → Selesaikan verifikasi yang diperlukan (catatan: warga AS saat ini diblokir)
3. **Deposit USDC** → Beli USDC di exchange utama (Coinbase, Kraken, Binance), transfer ke wallet Polygon
4. **Cari market** → Browse homepage atau topik yang Anda kenal baik
5. **Analisis market** → Bandingkan harga saat ini dengan estimasi probabilitas Anda
6. **Tempatkan trade** → Klik "Buy Yes" atau "Buy No", masukkan jumlah dollar
7. **Monitor posisi** → Posisi muncul di portfolio Anda
8. **Kumpulkan kemenangan** → Jika prediksi benar, shares redeemable otomatis di $1.00

### Tips Memulai

- Mulai dengan $50-100
- Pilih 3-5 market yang Anda kenal baik
- Pertahankan posisi individual kecil ($10-20 masing-masing)
- Fokus mempelajari mekanisme sebelum mengoptimalkan profit

### Discovery Harga

- Setiap market dimulai di 50 sen per share (50% probabilitas)
- Saat lebih banyak informasi tersedia dan trader membeli/menjual, harga berubah
- Pergerakan harga = agregasi informasi

---

## Jenis Order

### Market Order

| Fitur | Detail |
|-------|--------|
| **Eksekusi** | Segera di harga terbaik yang tersedia |
| **Kelebihan** | Cepat masuk, tidak perlu menunggu |
| **Kekurangan** | Slippage mungkin terjadi |
| **Contoh** | Beli 100 saham YES di $0.55 |
| **Fee** | 2% taker fee |

### Limit Order

| Fitur | Detail |
|-------|--------|
| **Eksekusi** | Set harga spesifik, tunggu eksekusi |
| **Kelebihan** | Mengurangi slippage untuk order besar |
| **Kekurangan** | Mungkin tidak terisi jika harga tidak mencapai level Anda |
| **Fee** | 0% maker fee |

**Pro Tips:**
- Gunakan limit order untuk menjadi Maker dan hindari fee 2%
- Sangat berharga untuk trade besar
- Jika market mengatakan 60% Yes tapi Anda pikir sebenarnya 75%, itu potensi edge untuk beli saham Yes

---

## Struktur Biaya

| Jenis Fee | Rate | Deskripsi |
|-----------|------|-----------|
| **Maker fee** | 0% | Memberikan likuiditas dengan limit order |
| **Taker fee** | 2% | Menghapus likuiditas dengan market order |
| **Gas fee** | ~$0.01-$0.10 | Biaya jaringan Polygon per transaksi |

### Detail Gas Fees

- Biaya Polygon sangat murah (<$0.01 per transaksi)
- Polymarket sering mensubsidi gas untuk operasi buy/sell standar
- Sebagian besar pengguna tidak perlu memikirkannya
- Memerlukan sedikit MATIC di wallet untuk operasi tertentu
- Platform biasanya menyediakan airdrop MATIC kecil untuk gas awal

---

## Proses Settlement

### Alur Settlement

1. Event terjadi → Polymarket mengambil hasil resmi
2. Oracle (UMA) memverifikasi hasil
3. Winning shares redeem otomatis di $1.00/share
4. Losing shares menjadi tidak bernilai
5. Dana otomatis kembali ke wallet Anda

### Timeline Resolusi

- Umumnya cepat dan otomatis - dalam beberapa jam setelah event selesai
- Kriteria resolusi ditentukan sebelum market dibuka
- Dapat dilihat publik, tidak ada kejutan tentang apa yang dihitung sebagai kemenangan

### Oracle System

| Komponen | Fungsi |
|----------|--------|
| **UMA Protocol** | Oracle utama untuk dispute resolution |
| **Challenge Window** | Pemegang UMA token dapat melakukan dispute dalam window ini |
| **Resolution Criteria** | Ditentukan sebelum market dibuka, dapat dilihat publik |

**Penting:** Sesekali market menghadapi dispute ketika hasil ambigu. Baca kriteria resolusi dengan hati-hati sebelum trading, terutama di market dengan kondisi kompleks seperti "X akan terjadi sebelum tanggal Y."

---

## Infrastruktur Teknis

### Blockchain Stack

| Komponen | Detail |
|----------|--------|
| **Network** | Polygon (Layer-2 di atas Ethereum) |
| **Currency** | USDC (USD Coin, pegged 1:1 ke USD) |
| **Gas Token** | MATIC (Polygon native token) |
| **Smart Contracts** | Gnosis Conditional Tokens Framework (CTF) |

### Kenapa Polygon?

- Transaksi sangat cepat (detik)
- Biaya gas sangat rendah (<$0.01)
- Kompatibel dengan Ethereum
- Transparansi penuh on-chain

---

## Dokumentasi API

### Tiga Lapisan API

| API | Tujuan | Auth Required? | Base URL |
|-----|--------|----------------|----------|
| **Gamma** | Market metadata, events, search | Tidak | gamma-api.polymarket.com |
| **CLOB** | Order book, trading, positions | Ya (wallet) | clob.polymarket.com |
| **Data** | Historical activity, profiles | Tidak | data-api.polymarket.com |

### Gamma API - Market Data & Discovery

Gateway entry point untuk menemukan market, mengambil metadata, dan membaca harga. Tidak perlu autentikasi untuk akses read-only.

**Key Endpoints:**

| Endpoint | Deskripsi |
|----------|-----------|
| `GET /markets` | List semua market dengan filter (active, closed, tags) |
| `GET /markets/{id}` | Detail single market termasuk harga, volume, outcomes |
| `GET /events` | List events (grup market terkait) |
| `GET /events/{id}` | Single event dengan semua market terkait |

**Contoh: Fetch Active Markets**
```bash
curl "https://gamma-api.polymarket.com/markets?closed=false&limit=10"

# Response includes:
# - id, question, description
# - outcomePrices (current Yes/No prices)
# - volume, liquidity
# - endDate, active status
```

### CLOB API - Order Book & Trading

Untuk menempatkan trade, mengelola order, dan membaca order book live. Memerlukan autentikasi berbasis wallet.

**Key Endpoints:**

| Endpoint | Deskripsi |
|----------|-----------|
| `GET /order-book/{token_id}` | Order book live (bids dan asks) untuk outcome token |
| `POST /order` | Tempatkan order baru (memerlukan autentikasi) |
| `DELETE /order/{id}` | Batalkan order terbuka |
| `GET /orders` | List order Anda (open dan filled) |
| `GET /trades` | Riwayat trade terbaru untuk market |

**Contoh: Baca Order Book**
```bash
curl "https://clob.polymarket.com/order-book/71321045679252212594626385532706912750332728571942532289631379312455583992563"

# Response:
# {
#   "bids": [{ "price": "0.55", "size": "1500.00" }, ...],
#   "asks": [{ "price": "0.56", "size": "800.00" }, ...],
#   "spread": "0.01",
#   "midpoint": "0.555"
# }
```

### Data API - Historical Activity

Menyediakan data trade historis, aktivitas pengguna, dan performa market. Berguna untuk backtesting strategi dan membangun analytics.

**Key Endpoints:**

| Endpoint | Deskripsi |
|----------|-----------|
| `GET /activity` | Feed aktivitas global terbaru (trades, market creations) |
| `GET /activity/{address}` | Aktivitas untuk wallet address tertentu |
| `GET /positions/{address}` | Posisi current yang dipegang oleh wallet |
| `GET /time-series/{token_id}` | Seri harga historis untuk outcome token |

### Autentikasi

**Wallet-Based Authentication (bukan API key tradisional):**

1. **Generate API credentials**
   - Derive API key dari Ethereum/Polygon wallet dengan menandatangani pesan
   - Membuat pasangan public/private key yang terikat ke wallet address

2. **Sign requests**
   - Setiap request API menyertakan signature header (POLY_HMAC_SHA256)
   - Generated dari API secret, timestamp request, dan body request

3. **L1 vs L2 auth**
   - **L1 authentication** (dari wallet utama) → full trading access
   - **L2 authentication** (dari derived key) → read-only access, digunakan oleh analytics tools

**Contoh Python (py-clob-client):**
```python
from py_clob_client import ClobClient

# Initialize dengan private key wallet Anda
client = ClobClient(
    host="https://clob.polymarket.com",
    key="YOUR_PRIVATE_KEY",
    chain_id=137  # Polygon mainnet
)

# Derive API credentials (one-time setup)
api_creds = client.create_or_derive_api_creds()
client.set_api_creds(api_creds)

# Sekarang Anda bisa place orders
order = client.create_and_post_order(
    token_id="71321045...",  # YES token untuk market
    price=0.55,
    size=100,
    side="BUY"
)
```

### WebSocket Feeds

Untuk data real-time, Polymarket menyediakan WebSocket feeds yang push update tanpa polling:

| Feed | Deskripsi |
|------|-----------|
| **Price Updates** | Perubahan harga real-time untuk market yang di-subscribe |
| **Order Book** | Diff order book live (new orders, cancellations, fills) untuk bot market-making |
| **Trades** | Feed eksekusi trade real-time. Berguna untuk volume tracking dan alerts |
| **User Events** | Update status order personal (fills, cancellations) untuk authenticated users |

**Contoh: Subscribe to Price Updates**
```javascript
// JavaScript WebSocket example
const ws = new WebSocket("wss://ws-subscriptions-clob.polymarket.com/ws/market");

ws.onopen = () => {
  ws.send(JSON.stringify({
    type: "market",
    assets_id: "71321045..."  // Subscribe ke token tertentu
  }));
};

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  console.log("Price update:", data);
  // { price: "0.56", timestamp: "2026-03-16T...", ... }
};
```

### Rate Limits

| API | Rate Limit | Catatan |
|-----|------------|---------|
| Gamma | ~60 req/min | Unauthenticated; gunakan caching untuk query berulang |
| CLOB (read) | ~100 req/min | Authenticated; gunakan WebSockets untuk data real-time |
| CLOB (write) | ~30 req/min | Order placement/cancellation; batch operations jika memungkinkan |
| WebSocket | 5 connections/IP | Multiplex subscriptions melalui satu koneksi |

**Best Practices:**
- **Gunakan WebSockets** untuk data real-time alih-alih polling REST endpoints
- **Cache Gamma responses** — metadata market jarang berubah
- **Implementasikan exponential backoff** saat menerima 429 (rate limited) responses
- **Batch operations** dimungkinkan untuk mengurangi jumlah request
- **Gunakan Polygon RPC** sebagai fallback untuk data on-chain jika API down

### Contoh Lengkap: Fetch Active Markets

**Python:**
```python
import requests

# Step 1: Fetch active markets dari Gamma API
response = requests.get(
    "https://gamma-api.polymarket.com/markets",
    params={"closed": "false", "limit": 5}
)
markets = response.json()

for market in markets:
    question = market["question"]
    yes_price = market["outcomePrices"][0]
    no_price = market["outcomePrices"][1]
    volume = float(market.get("volume", 0))
    print(question)
    print("  Yes:", yes_price, " No:", no_price)
    print("  Volume: $" + str(int(volume)))

# Step 2: Fetch order book untuk YES token market pertama
token_id = markets[0]["clobTokenIds"][0]  # YES token ID
book = requests.get(
    "https://clob.polymarket.com/order-book/" + token_id
).json()

print("Best bid:", book["bids"][0]["price"])
print("Best ask:", book["asks"][0]["price"])
print("Spread:", book.get("spread", "N/A"))
```

**JavaScript (Node.js):**
```javascript
// Fetch active markets dari Gamma API
const res = await fetch(
  "https://gamma-api.polymarket.com/markets?closed=false&limit=5"
);
const markets = await res.json();

for (const market of markets) {
  console.log(market.question);
  console.log(`  Yes: ${market.outcomePrices[0]}  No: ${market.outcomePrices[1]}`);
}

// Fetch order book untuk market pertama
const tokenId = markets[0].clobTokenIds[0];
const book = await fetch(
  `https://clob.polymarket.com/order-book/${tokenId}`
).then(r => r.json());

console.log(`Best bid: ${book.bids[0]?.price}`);
console.log(`Best ask: ${book.asks[0]?.price}`);
```

### Ecosystem Tools

Tools dan proyek yang membangun di atas API Polymarket:

| Tool | Tipe | Deskripsi |
|------|------|-----------|
| **py-clob-client** | SDK | Official Python SDK untuk CLOB API. Handle autentikasi, order placement, WebSocket subscriptions |
| **Polymarket Subgraph** | Data | GraphQL API (The Graph) untuk query data on-chain: trades, positions, market state |
| **Trading Terminals** | Tool | Third-party terminals seperti Polydex menawarkan UIs enhanced built on CLOB API |
| **Wallet Trackers** | Analytics | Tools yang menggunakan Data API untuk track whale wallets, copy trades, dan analisis strategi profitable |

---

## Setup Wallet

### Wallet yang Didukung

| Wallet | Tipe | Keterangan |
|--------|------|------------|
| **MetaMask** | Non-custodial | Paling populer, desktop (browser extension) dan mobile |
| **Coinbase Wallet** | Non-custodial | Pilihan alternatif |
| **WalletConnect** | Non-custodial | Kompatibel dengan banyak wallet |
| **Custodial Option** | Custodial | Login dengan email, wallet dikelola oleh platform (lebih sederhana untuk pemula, tapi Anda tidak kontrol key) |

### Panduan Setup

1. **Buat Wallet** → Instal MetaMask atau wallet lain
2. **Simpan Seed Phrase** → CATATAN PENTING: Jangan pernah bagikan private key/seed phrase
3. **Switch ke Polygon Network** → Tambahkan Polygon mainnet ke wallet
4. **Deposit USDC** → Beli USDC di exchange, transfer ke wallet address
5. **Deposit MATIC kecil** → Untuk gas fees (biasanya disediakan platform)

### Recovery

- Jika signup via Magic Link atau proxy wallet, recovery mungkin dimungkinkan melalui [recovery.polymarket.com](https://recovery.polymarket.com)

---

## Peringatan Risiko

### Risiko Finansial

| Risiko | Deskripsi |
|--------|-----------|
| **Market Risk** | Anda bisa kehilangan selurui posisi jika prediksi salah. Jangan trade uang yang tidak bisa Anda tanggung kerugiannya |
| **Liquidity Risk** | Di market volume rendah, spread antara harga beli dan jual bisa lebar, dan exit lebih awal mungkin mahal |
| **Resolution Risk** | Dispute tentang hasil bisa menunda atau mengubah pembayaran. Baca kriteria resolusi dengan hati-hati |
| **Smart Contract Risk** | Polymarket telah diaudit, tapi tidak ada smart contract 100% kebal bug. Jangan taruh tabungan hidup di satu platform |
| **Regulatory Risk** | Hukum seputar prediction market terus berkembang. Polymarket memblokir pengguna AS, tapi regulasi di tempat lain bisa berubah |

### Pertimbangan Penting

- **Jangan pernah investasi lebih dari yang Anda mampu untuk kehilangan**
- Market dengan likuiditas rendah mungkin sulit untuk keluar
- Settlement bisa tertunda (menunggu konfirmasi oracle)
- Baca kriteria resolusi dengan hati-hati sebelum trading
- **Warga AS saat ini diblokir dari platform**

### Kesalahan Pemula yang Harus Dihindari

| Kesalahan | Solusi |
|-----------|--------|
| Bertaruh seluruh saldo pada satu market | Diversifikasi across beberapa market |
| Mengabaikan likuiditas | Periksa volume sebelum trading |
| Trading berdasarkan emosi | Jika harga turun, tidak selalu berarti Anda harus jual. Kembali ke data |
| Tidak memahami aturan resolusi | Baca deskripsi market dengan hati-hati |

---

## Strategi Trading

### Prinsip Utama

1. **Ikuti data, bukan opini** → Pedagang yang menguntungkan mengandalkan data, bukan perasaan
2. **Track wallet profitable** → Lihat apa yang trader terbaik beli dan jual
3. **Gunakan Smart Feed** → Trade real-time dari pemenang terverifikasi
4. **Setup alerts** → Jangan pernah ketinggalan saat wallet yang di-track melakukan langkah
5. **Copy trade** → Secara otomatis replikasi trade mereka

### Pendekatan yang Direkomendasikan untuk Pemula

```
Mulai dengan: $50-100
Pilih: 3-5 market yang Anda kenal baik
Posisi individual: $10-20 masing-masing
Fokus: Mempelajari mekanisme sebelum mengoptimalkan profit
```

### Strategi Lanjutan (untuk dipelajari nanti)

- **Arbitrage** → Memanfaatkan perbedaan harga antar platform
- **Market Making** → Memberikan likuiditas untuk mendapatkan fee
- **Event-Driven Trading** → Trading berdasarkan analisis fundamental
- **Copy Trading** → Mengikuti trader profitable secara otomatis

---

## Sumber Daya

### Link Resmi

| Resource | URL |
|----------|-----|
| **Polymarket Official Site** | https://polymarket.com |
| **Polymarket Developer Docs** | https://docs.polymarket.com |
| **Polymarket Help Center** | https://help.polymarket.com |
| **Polymarket GitHub** | https://github.com/Polymarket |
| **Recovery Portal** | https://recovery.polymarket.com |

### Developer Tools

| Tool | URL | Deskripsi |
|------|-----|-----------|
| **py-clob-client** | https://github.com/Polymarket/py-clob-client | Official Python SDK untuk CLOB API |
| **Polymarket Subgraph** | https://github.com/Polymarket | GraphQL API untuk data on-chain |
| **Gnosis CTF Contracts** | https://github.com/gnosis/conditional-tokens-contracts | Smart contracts yang mendasari market Polymarket |

### Learning Resources

| Resource | URL |
|----------|-----|
| **Polymarket 101** | https://docs.polymarket.com/polymarket-101 |
| **Polymarket API Guide** | https://pm.wiki/learn/polymarket-api |
| **Polymarket Basics** | https://www.pred101.com/en/knowledge-base/tutorials/polymarket-basics |
| **How Polymarket Works** | https://learnpolymarket.com/blog/how-polymarket-works |
| **Polymarket Book** | https://polymarket-book.com |

### Tools Ekosistem

| Tool | Kategori | Deskripsi |
|------|----------|-----------|
| **PolymarketScan** | Analytics | Analytics dan whale tracking |
| **Polycool** | Trading | Telegram bot untuk trading |
| **Hunch** | Analytics | Prediksi dan analisis |

---

## FAQ

### Apakah API Polymarket gratis?
Ya. Ketiga API Polymarket (Gamma, CLOB, dan Data) gratis digunakan. Tidak perlu API key untuk akses read-only market data via Gamma API. CLOB API memerlukan autentikasi berbasis wallet untuk menempatkan orders, tapi tidak ada fee penggunaan selain trading fees standar (0% di sebagian besar market).

### Apakah saya memerlukan API key untuk Polymarket?
Tidak perlu API key tradisional. Gamma API (market data, events, harga) sepenuhnya publik tanpa autentikasi. CLOB API menggunakan autentikasi berbasis wallet — Anda menandatangani pesan dengan Ethereum/Polygon wallet untuk autentikasi. Ini menggantikan model API key tradisional.

### Bisakah saya mengotomatiskan trading di Polymarket?
Ya. CLOB API mendukung full automated trading: menempatkan limit orders, market orders, membatalkan orders, dan mengelola posisi secara programatis. Banyak trader menjalankan bot Python atau JavaScript yang berinteraksi dengan CLOB API. Polymarket juga menyediakan official Python SDK (py-clob-client) untuk tujuan ini.

### Bahasa pemrograman apa yang bisa saya gunakan dengan API Polymarket?
API adalah REST endpoints standar yang bekerja dengan bahasa apa pun. Polymarket menyediakan official Python SDK (py-clob-client) dan ada community TypeScript/JavaScript libraries. Anda juga bisa menggunakan curl, Go, Rust, atau bahasa apa pun dengan dukungan HTTP. WebSocket feeds menggunakan protokol WebSocket standar.

### Apa rate limit API Polymarket?
Gamma API mengizinkan sekitar 60 requests per menit untuk akses unauthenticated. CLOB API memiliki limit lebih tinggi untuk authenticated users — sekitar 100+ requests per menit untuk order management. WebSocket connections memiliki limit 5 concurrent connections per IP. Selalu implementasikan exponential backoff untuk 429 responses.

---

*Terakhir Diperbarui: Juni 2026*
*Sumber: Polymarket Documentation, pred101.com, learnpolymarket.com, pm.wiki*
*Catatan: Dokumentasi ini dikumpulkan menggunakan browser dari sumber-sumber terpercaya*
