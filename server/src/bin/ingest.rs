#[path = "../db.rs"]
mod db;
use sqlx::PgPool;
use std::{env, fs};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        eprintln!(
            "Usage: cargo run -p server --bin ingest -- <url1> <url2> ... | --file urls.txt"
        );
        std::process::exit(1);
    }

    // Gather URLs from --file or positional args
    let mut urls: Vec<String> = Vec::new();
    if args.len() >= 2 && args[0] == "--file" {
        let _flag = args.remove(0);
        let file_path = args.remove(0);
        let content = fs::read_to_string(&file_path)?;
        for line in content.lines() {
            // Trim whitespace and strip inline comments (anything after '#')
            let mut line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            if let Some((head, _)) = line.split_once('#') { line = head.trim(); }
            if line.is_empty() { continue; }
            urls.push(line.to_string());
        }
    } else {
        urls = args;
    }

    if urls.is_empty() {
        eprintln!("No URLs provided");
        std::process::exit(1);
    }

    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set for ingestion");
    let pool = db::connect(&database_url).await?;

    let client = reqwest::Client::builder()
        .user_agent("rracer-ingest/0.1")
        .timeout(std::time::Duration::from_secs(20))
        .build()?;

    let mut total_inserted = 0usize;

    for url in urls {
        match fetch_and_extract(&client, &url).await {
            Ok(passages) => {
                info!("Fetched {} passages from {}", passages.len(), url);
                let inserted = insert_passages(&pool, &url, &passages).await?;
                total_inserted += inserted;
                info!("Inserted {} new passages from {}", inserted, url);
            }
            Err(e) => {
                warn!("Failed to fetch {}: {:?}", url, e);
            }
        }
    }

    info!("Total inserted: {}", total_inserted);
    Ok(())
}

async fn fetch_and_extract(client: &reqwest::Client, url: &str) -> anyhow::Result<Vec<String>> {
    let resp = client.get(url).send().await?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("HTTP {}", status);
    }
    let body = resp.text().await?;
    let passages = extract_passages_from_html(&body);
    Ok(passages)
}

fn extract_passages_from_html(html: &str) -> Vec<String> {
    use scraper::{Html, Selector};
    let doc = Html::parse_document(html);
    let p_sel = Selector::parse("p").unwrap();
    let raw_paras: Vec<String> = doc
        .select(&p_sel)
        .map(|p| normalize_space(&p.text().collect::<String>()))
        .filter(|t| t.len() > 80)
        .collect();

    // Combine paragraphs into medium-length passages
    let min_len = 220usize;
    let max_len = 650usize;
    let mut out = Vec::new();
    let mut buf = String::new();

    for para in raw_paras {
        if para.len() > max_len {
            // Split long paragraphs by sentence boundary heuristics
            for chunk in split_sentences(&para, max_len) {
                push_chunk(&mut out, &mut buf, chunk, min_len, max_len);
            }
        } else {
            push_chunk(&mut out, &mut buf, para, min_len, max_len);
        }
    }

    if !buf.is_empty() && buf.len() >= min_len {
        out.push(buf.trim().to_string());
    }

    // Final filtering: ensure passages have letters and end with punctuation
    out.into_iter()
        .map(|mut s| {
            if !matches!(s.chars().last(), Some('.') | Some('!') | Some('?')) {
                s.push('.');
            }
            s
        })
        .filter(|s| s.chars().any(|c| c.is_alphabetic()))
        .collect()
}

fn push_chunk(out: &mut Vec<String>, buf: &mut String, next: String, min_len: usize, max_len: usize) {
    let cur_len = buf.len();
    if cur_len == 0 {
        buf.push_str(&next);
    } else if cur_len + 1 + next.len() <= max_len {
        buf.push(' ');
        buf.push_str(&next);
    } else {
        if cur_len >= min_len {
            out.push(buf.trim().to_string());
        }
        buf.clear();
        buf.push_str(&next);
    }
}

fn split_sentences(long: &str, max_len: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for sent in long.split(&['.', '!', '?'][..]) {
        let s = normalize_space(sent);
        if s.is_empty() { continue; }
        if cur.len() + s.len() + 1 > max_len {
            if !cur.is_empty() { out.push(cur.trim().to_string()); }
            cur = s;
        } else {
            if !cur.is_empty() { cur.push(' '); }
            cur.push_str(&s);
        }
    }
    if !cur.is_empty() { out.push(cur.trim().to_string()); }
    out
}

fn normalize_space(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            last_space = false;
            out.push(c);
        }
    }
    out.trim().to_string()
}

async fn insert_passages(pool: &PgPool, source_url: &str, passages: &[String]) -> anyhow::Result<usize> {
    let mut inserted = 0usize;
    for text in passages {
        if text.len() < 120 { continue; }
        let res = sqlx::query(
            r#"INSERT INTO passages (text, source_url) VALUES ($1, $2)
                ON CONFLICT (text) DO NOTHING"#,
        )
        .bind(text)
        .bind(source_url)
        .execute(pool)
        .await?;
        inserted += res.rows_affected() as usize;
    }
    Ok(inserted)
}
