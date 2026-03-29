use worker::*;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::cmp::Ordering;
use std::collections::HashMap;

// 1. Data Model
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub win_rate: f32,
    pub rating_rank: u32,
    pub member_id: String,
    pub player_name: String, 
    pub club_doc_id: Option<u32>,
    pub club_name: Option<String>,
    pub address: Option<String>,
    pub gender: Option<String>,
    pub country: Option<String>,
    #[serde(alias = "ttr")]
    pub final_rating: u32,
    pub total_matches: u32,
    pub total_wins: u32,
    pub total_lost: u32,
}

// 2. Optimized RAM Storage
struct IndexedPlayer {
    original: Player,
    tokens: Box<[Box<str>]>, 
}

struct SearchDatabase {
    players: Box<[IndexedPlayer]>,
    // Inverted index for names: Token -> List of Player indices
    name_index: HashMap<Box<str>, Vec<u32>>,
    // Exact ID index: ID -> Player index
    id_map: HashMap<Box<str>, u32>,
}

static DB: OnceLock<SearchDatabase> = OnceLock::new();

// 3. The Core Search Math
#[inline(always)]
fn score_match(query_tokens: &[&str], db_tokens: &[Box<str>]) -> f64 {
    if query_tokens.is_empty() || db_tokens.is_empty() {
        return 0.0;
    }

    let mut total_score = 0.0;
    let query_len_recip = 1.0 / (query_tokens.len() as f64);

    for q_token in query_tokens {
        let mut best_token_score = 0.0;
        
        for db_token in db_tokens {
            let db_ref = db_token.as_ref();
            if db_ref == *q_token {
                best_token_score = 1.0;
                break;
            } 
            
            if db_ref.starts_with(q_token) {
                let score = 0.99 - ((db_ref.len() - q_token.len()) as f64 * 0.01);
                if score > best_token_score {
                    best_token_score = score;
                }
            } else if q_token.len() >= 3 {
                let score = strsim::jaro_winkler(q_token, db_ref);
                if score > best_token_score {
                    best_token_score = score;
                }
            }

            if best_token_score > 0.98 {
                break;
            }
        }
        total_score += best_token_score;
    }

    total_score * query_len_recip
}

// 4. Inverted Index Lookup (Name)
fn name_lookup(db: &SearchDatabase, query_tokens: &[&str]) -> Vec<u32> {
    let mut candidates = HashMap::new();
    for token in query_tokens {
        if let Some(indices) = db.name_index.get(*token as &str) {
            for &idx in indices {
                *candidates.entry(idx).or_insert(0) += 1;
            }
        }
    }
    let mut result: Vec<(u32, i32)> = candidates.into_iter().collect();
    result.sort_unstable_by(|a, b| b.1.cmp(&a.1));
    result.into_iter().map(|(idx, _)| idx).collect()
}

// 5. The Request Handler
#[event(fetch)]
pub async fn main(req: Request, _env: Env, _ctx: worker::Context) -> Result<Response> {
    let db = DB.get_or_init(|| {
        let json_data = include_str!("./usatt_player_rankings.json");
        let players_raw: Vec<Player> = serde_json::from_str(json_data).expect("Invalid JSON");
        
        let mut name_index: HashMap<Box<str>, Vec<u32>> = HashMap::with_capacity(players_raw.len() * 2);
        let mut id_map: HashMap<Box<str>, u32> = HashMap::with_capacity(players_raw.len());
        
        let indexed_players: Vec<IndexedPlayer> = players_raw.into_iter().enumerate().map(|(idx, p)| {
            // Index ID
            id_map.insert(p.member_id.clone().into(), idx as u32);
            
            // Tokenize and Index Name
            let tokens: Vec<Box<str>> = p.player_name
                .to_lowercase()
                .split_whitespace()
                .map(|s| {
                    let boxed: Box<str> = s.into();
                    name_index.entry(boxed.clone()).or_default().push(idx as u32);
                    boxed
                })
                .collect();

            IndexedPlayer {
                original: p,
                tokens: tokens.into_boxed_slice(),
            }
        }).collect();

        SearchDatabase {
            players: indexed_players.into_boxed_slice(),
            name_index,
            id_map,
        }
    });

    if req.method() != Method::Get {
        return Response::error("Method Not Allowed", 405);
    }

    let url = req.url()?;
    let query_param = url.query_pairs()
        .find(|(k, _)| k == "q")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default();

    // Sanitize and check length (3-32)
    let q = query_param.trim();
    if q.len() < 3 || q.len() > 32 {
        return Response::from_json(&Vec::<Player>::new());
    }

    let is_numeric = q.chars().all(|c| c.is_ascii_digit());
    let mut results: Vec<&Player> = Vec::with_capacity(3);

    if is_numeric {
        // ID Search Strategy
        if let Some(&idx) = db.id_map.get(q) {
            results.push(&db.players[idx as usize].original);
        } else {
            for indexed in db.players.iter() {
                if indexed.original.member_id.starts_with(q) {
                    results.push(&indexed.original);
                    if results.len() >= 3 { break; }
                }
            }
        }
    } else {
        // Name Search Strategy
        let lower_q = q.to_lowercase();
        let query_tokens: Vec<&str> = lower_q.split_whitespace().collect();
        let candidate_indices = name_lookup(db, &query_tokens);
        
        struct ScoredPlayer<'a> {
            player: &'a Player,
            score: f64,
        }
        let mut scored_results = Vec::new();
        let mut top_indexed_score = 0.0;

        if !candidate_indices.is_empty() {
            for &idx in &candidate_indices {
                let player = &db.players[idx as usize];
                let score = score_match(&query_tokens, &player.tokens);
                if score > 0.75 {
                    if score > top_indexed_score {
                        top_indexed_score = score;
                    }
                    scored_results.push(ScoredPlayer {
                        player: &player.original,
                        score,
                    });
                }
            }
        }

        // Satisfaction Guardrail: If no perfect match found via index, fallback to full scan.
        if scored_results.len() < 3 || top_indexed_score < 0.95 {
            let candidate_set: std::collections::HashSet<u32> = candidate_indices.iter().copied().collect();
            let mut full_scan: Vec<ScoredPlayer> = db.players.iter()
                .enumerate()
                .filter(|(idx, _)| !candidate_set.contains(&(*idx as u32)))
                .map(|(_, indexed)| ScoredPlayer {
                    player: &indexed.original,
                    score: score_match(&query_tokens, &indexed.tokens),
                })
                .filter(|sp| sp.score > 0.75)
                .collect();
            scored_results.append(&mut full_scan);
        }

        scored_results.sort_unstable_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

        if let Some(first) = scored_results.first() {
            let top_score = first.score;
            for sp in scored_results.into_iter().take(3) {
                if (top_score - sp.score) <= 0.3 {
                    results.push(sp.player);
                } else {
                    break;
                }
            }
        }
    }

    let headers = Headers::new();
    let mut response = Response::from_json(&results)?.with_headers(headers);
    let headers_mut = response.headers_mut();
    headers_mut.set("Access-Control-Allow-Origin", "*")?;
    headers_mut.set("Content-Type", "application/json")?;
    headers_mut.set("Vary", "Origin")?;
    headers_mut.set("Cache-Control", "public, max-age=3600")?;

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_real_db() -> SearchDatabase {
        let json_data = include_str!("./usatt_player_rankings.json");
        let players_raw: Vec<Player> = serde_json::from_str(json_data).expect("Invalid JSON");
        
        let mut name_index: HashMap<Box<str>, Vec<u32>> = HashMap::with_capacity(players_raw.len() * 2);
        let mut id_map: HashMap<Box<str>, u32> = HashMap::with_capacity(players_raw.len());
        
        let indexed_players: Vec<IndexedPlayer> = players_raw.into_iter().enumerate().map(|(idx, p)| {
            id_map.insert(p.member_id.clone().into(), idx as u32);
            let tokens: Vec<Box<str>> = p.player_name
                .to_lowercase()
                .split_whitespace()
                .map(|s| {
                    let boxed: Box<str> = s.into();
                    name_index.entry(boxed.clone()).or_default().push(idx as u32);
                    boxed
                })
                .collect();

            IndexedPlayer {
                original: p,
                tokens: tokens.into_boxed_slice(),
            }
        }).collect();

        SearchDatabase {
            players: indexed_players.into_boxed_slice(),
            name_index,
            id_map,
        }
    }

    #[test]
    fn test_id_search_exact() {
        let db = get_real_db();
        let query = "72193"; 
        assert!(db.id_map.get(query).is_some());
    }

    #[test]
    fn test_id_search_prefix() {
        let db = get_real_db();
        let query = "7219"; 
        let matches: Vec<&Player> = db.players.iter()
            .filter(|p| p.original.member_id.starts_with(query))
            .map(|p| &p.original)
            .collect();
        assert!(!matches.is_empty());
    }

    #[test]
    fn test_satisfaction_guardrail() {
        let db = get_real_db();
        // "kank" is NOT in the index, but "kanak" is in the DB.
        // This test ensures the guardrail triggers the full scan.
        let query = vec!["kank"];
        let candidates = name_lookup(&db, &query);
        assert!(candidates.is_empty()); // Verify index missed it
        
        let mut found = false;
        for p in db.players.iter() {
            let score = score_match(&query, &p.tokens);
            if score > 0.8 && p.original.player_name == "Kanak Jha" {
                found = true;
                break;
            }
        }
        assert!(found);
    }
}
