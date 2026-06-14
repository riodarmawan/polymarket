use crate::models::probability::Signal;

pub fn generate_signals(
    news_sentiment: f64,
    polling_data: f64,
    expert_opinion: f64,
    historical_trend: f64,
) -> Vec<Signal> {
    vec![
        Signal {
            name: "news".to_string(),
            value: news_sentiment,
            confidence: 0.8,
        },
        Signal {
            name: "polling".to_string(),
            value: polling_data,
            confidence: 0.6,
        },
        Signal {
            name: "expert".to_string(),
            value: expert_opinion,
            confidence: 0.7,
        },
        Signal {
            name: "historical".to_string(),
            value: historical_trend,
            confidence: 0.5,
        },
    ]
}
