use serde::{Deserialize, Serialize};

// ── Flexible number helper ─────────────────────────────────

pub mod flex {
    use serde::{Deserialize, Deserializer};

    pub fn f64<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum OneOf {
            Num(f64),
            Str(String),
        }
        match OneOf::deserialize(d)? {
            OneOf::Num(n) => Ok(n),
            OneOf::Str(s) => s.parse::<f64>().map_err(serde::de::Error::custom),
        }
    }

    pub fn opt_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum OneOf {
            Null,
            Num(f64),
            Str(String),
        }
        match OneOf::deserialize(d)? {
            OneOf::Null => Ok(None),
            OneOf::Num(n) => Ok(Some(n)),
            OneOf::Str(s) => {
                let n = s.parse::<f64>().map_err(serde::de::Error::custom)?;
                Ok(Some(n))
            }
        }
    }
}

// ── Event ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    #[serde(default)]
    pub ticker: Option<String>,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    pub active: bool,
    pub closed: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub new: bool,
    #[serde(default)]
    pub featured: bool,
    #[serde(default)]
    pub restricted: bool,
    #[serde(default, deserialize_with = "flex::f64")]
    pub volume: f64,
    #[serde(default, deserialize_with = "flex::f64")]
    pub volume_24hr: f64,
    #[serde(default, deserialize_with = "flex::f64")]
    pub liquidity: f64,
    #[serde(default, deserialize_with = "flex::f64")]
    pub open_interest: f64,
    #[serde(default)]
    pub comment_count: i64,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<TagInfo>,
    #[serde(default)]
    pub markets: Vec<Market>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

// ── Market ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Market {
    pub id: String,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub slug: String,
    #[serde(default)]
    pub condition_id: Option<String>,
    #[serde(default)]
    pub outcomes: Option<String>,
    #[serde(default)]
    pub outcome_prices: Option<String>,
    #[serde(default)]
    pub enable_order_book: bool,
    #[serde(default)]
    pub active: Option<bool>,
    #[serde(default)]
    pub closed: Option<bool>,
    #[serde(default, deserialize_with = "flex::f64")]
    pub volume: f64,
    #[serde(default, deserialize_with = "flex::f64")]
    pub liquidity: f64,
    #[serde(default, deserialize_with = "flex::f64")]
    pub open_interest: f64,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub tags: Vec<TagInfo>,
    #[serde(default)]
    pub tokens: Vec<Token>,
    #[serde(default)]
    pub fee: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub clob_token_ids: Option<String>,
}

// ── Token ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub token_id: String,
    pub outcome: String,
    #[serde(default)]
    pub price: Option<f64>,
}

// ── Tag ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagInfo {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default, deserialize_with = "flex::f64")]
    pub volume: f64,
    #[serde(default, deserialize_with = "flex::f64")]
    pub liquidity: f64,
    #[serde(default)]
    pub num_markets: Option<i64>,
}

// ── Search ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    #[serde(default)]
    pub events: Option<Vec<Event>>,
    #[serde(default)]
    pub markets: Option<Vec<Market>>,
    #[serde(default)]
    pub profiles: Option<Vec<serde_json::Value>>,
}
