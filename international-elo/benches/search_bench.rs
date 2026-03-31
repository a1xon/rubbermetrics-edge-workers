use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::collections::{HashMap, HashSet};

// Inline the types we need (can't import worker-dependent crate directly in bench)
use serde::Deserialize;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Player {
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

struct IndexedPlayer {
    original: Player,
    tokens: Box<[Box<str>]>,
}

struct SearchDatabase {
    players: Box<[IndexedPlayer]>,
    name_index: HashMap<Box<str>, Vec<u32>>,
    id_map: HashMap<Box<str>, u32>,
}

fn build_db() -> SearchDatabase {
    let json_data = include_str!("../src/usatt_player_rankings.json");
    let players_raw: Vec<Player> = serde_json::from_str(json_data).unwrap();
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
        IndexedPlayer { original: p, tokens: tokens.into_boxed_slice() }
    }).collect();
    SearchDatabase { players: indexed_players.into_boxed_slice(), name_index, id_map }
}

#[inline(always)]
fn score_match(query_tokens: &[&str], db_tokens: &[Box<str>]) -> f64 {
    if query_tokens.is_empty() || db_tokens.is_empty() { return 0.0; }
    let mut total_score = 0.0;
    let query_len_recip = 1.0 / (query_tokens.len() as f64);
    for q_token in query_tokens {
        let mut best_token_score = 0.0;
        for db_token in db_tokens {
            let db_ref = db_token.as_ref();
            if db_ref == *q_token { best_token_score = 1.0; break; }
            if db_ref.starts_with(q_token) {
                let score = 0.99 - ((db_ref.len() - q_token.len()) as f64 * 0.01);
                if score > best_token_score { best_token_score = score; }
            } else if q_token.len() >= 3 {
                let score = strsim::jaro_winkler(q_token, db_ref);
                if score > best_token_score { best_token_score = score; }
            }
            if best_token_score > 0.98 { break; }
        }
        total_score += best_token_score;
    }
    total_score * query_len_recip
}

fn name_lookup(db: &SearchDatabase, query_tokens: &[&str]) -> Vec<u32> {
    let mut candidates = HashMap::new();
    for token in query_tokens {
        if let Some(indices) = db.name_index.get(*token as &str) {
            for &idx in indices { *candidates.entry(idx).or_insert(0) += 1; }
        }
    }
    let mut result: Vec<(u32, i32)> = candidates.into_iter().collect();
    result.sort_unstable_by(|a, b| b.1.cmp(&a.1));
    result.into_iter().map(|(idx, _)| idx).collect()
}

// --- The two strategies being benchmarked ---

fn full_scan_vec(db: &SearchDatabase, query_tokens: &[&str], candidate_indices: &[u32]) -> usize {
    db.players.iter()
        .enumerate()
        .filter(|(idx, _)| !candidate_indices.contains(&(*idx as u32)))
        .map(|(_, indexed)| score_match(query_tokens, &indexed.tokens))
        .filter(|&s| s > 0.75)
        .count()
}

fn full_scan_hashset(db: &SearchDatabase, query_tokens: &[&str], candidate_indices: &[u32]) -> usize {
    let candidate_set: HashSet<u32> = candidate_indices.iter().copied().collect();
    db.players.iter()
        .enumerate()
        .filter(|(idx, _)| !candidate_set.contains(&(*idx as u32)))
        .map(|(_, indexed)| score_match(query_tokens, &indexed.tokens))
        .filter(|&s| s > 0.75)
        .count()
}

fn bench_full_scan(c: &mut Criterion) {
    let db = build_db();

    // Queries that trigger the guardrail: partial/fuzzy matches
    // "li" is a very common token → large candidate list
    // "kank" is not in the index → empty candidate list
    let cases: &[(&str, &str)] = &[
        ("common_token_li",    "li"),
        ("common_token_zhang", "zhang"),
        ("rare_token_kank",    "kank"),
        ("two_tokens",         "john smith"),
    ];

    let mut group = c.benchmark_group("full_scan");
    group.sample_size(20);

    for (label, query) in cases {
        let lower = query.to_lowercase();
        let query_tokens: Vec<&str> = lower.split_whitespace().collect();
        let candidates = name_lookup(&db, &query_tokens);

        println!("[{}] query={:?}  candidates={}", label, query, candidates.len());

        group.bench_with_input(BenchmarkId::new("vec_contains", label), label, |b, _| {
            b.iter(|| full_scan_vec(&db, &query_tokens, &candidates))
        });
        group.bench_with_input(BenchmarkId::new("hashset_lookup", label), label, |b, _| {
            b.iter(|| full_scan_hashset(&db, &query_tokens, &candidates))
        });
    }

    group.finish();
}

criterion_group!(benches, bench_full_scan);
criterion_main!(benches);
