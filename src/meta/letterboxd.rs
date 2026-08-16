use anyhow::Result;
use scraper::{Html, Selector};
use serde_json::Value;
use tokio::sync::Semaphore;

static REQUEST_LIMIT: Semaphore = Semaphore::const_new(1);

pub async fn search_score(title: &str, _year: Option<u32>) -> Result<Option<f32>> {
    let _permit = REQUEST_LIMIT.acquire().await?;
    let slug = title
        .to_lowercase()
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() {
                Some(c)
            } else if c.is_whitespace() || c == '-' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        return Ok(None);
    }

    let url = format!("https://letterboxd.com/film/{}/", slug);
    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .timeout(std::time::Duration::from_secs(5))
        .min_tls_version(reqwest::tls::Version::TLS_1_3)
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()?;

    let resp = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return Ok(None),
    };

    let html_text = match resp.text().await {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };

    let document = Html::parse_document(&html_text);
    let selector = match Selector::parse("script[type=\"application/ld+json\"]") {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };

    for element in document.select(&selector) {
        let json_text = element.text().collect::<Vec<_>>().join("");
        // Clean CDATA if present
        let clean_json = json_text
            .trim()
            .trim_start_matches("/* <![CDATA[ */")
            .trim_end_matches("/* ]]> */")
            .trim();

        if let Ok(v) = serde_json::from_str::<Value>(clean_json) {
            if let Some(rating_val) = v.get("aggregateRating").and_then(|r| r.get("ratingValue")) {
                if let Some(f) = rating_val.as_f64() {
                    return Ok(Some(f as f32));
                } else if let Some(s) = rating_val.as_str() {
                    if let Ok(f) = s.parse::<f32>() {
                        return Ok(Some(f));
                    }
                }
            }
        }
    }

    Ok(None)
}
