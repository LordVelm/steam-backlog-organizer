//! Taste engine core: taste vector, bounce detection, anti-clusters, candidate
//! scoring, deterministic reasons, and "more like this". Formulas are specced in
//! the v4.0 plan — change them only together with their tests.

use crate::catalog::{Catalog, CatalogGameMeta, EMBED_DIM};
use crate::classifier::Category;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const SECS_PER_DAY: u64 = 86_400;
const SECS_PER_YEAR: f64 = 365.25 * 86_400.0;

/// One owned game with every signal the taste engine needs. Assembled by the
/// caller (lib.rs) from library + classifications + HLTB + catalog vectors.
#[derive(Debug, Clone)]
pub struct GameSignal {
    pub appid: u64,
    pub name: String,
    pub hours: f64,
    pub hours_2weeks: f64,
    /// Unix seconds of last session; 0 = never/unknown.
    pub rtime_last_played: u64,
    /// Achievement completion 0-100 when known.
    pub ach_pct: Option<f64>,
    pub category: Category,
    pub hltb_main_hours: Option<f64>,
    /// Catalog (or runtime-embedded) vector; None = contributes no direction.
    pub vector: Option<[f32; EMBED_DIM]>,
    /// Catalog user tags (vote order).
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagAffinity {
    pub tag: String,
    /// Normalized 0-1 (top tag = 1.0).
    pub weight: f64,
    pub example_appids: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorGame {
    pub appid: u64,
    pub name: String,
    pub weight: f64,
    #[serde(skip)]
    pub vector: Option<[f32; EMBED_DIM]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BouncedGame {
    pub appid: u64,
    pub name: String,
    pub playtime_hours: f64,
    pub last_played: u64,
    /// "bounced" | "abandoned"
    pub kind: String,
}

fn zero_vec() -> [f32; EMBED_DIM] {
    [0.0; EMBED_DIM]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntiCluster {
    /// Highest-signal tag, e.g. "Competitive FPS".
    pub label: String,
    pub tags: Vec<String>,
    pub bounced: Vec<BouncedGame>,
    /// 0-1, saturates at 6 effective bounces.
    pub strength: f64,
    #[serde(skip, default = "zero_vec")]
    pub vector: [f32; EMBED_DIM],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteProfile {
    #[serde(skip, default = "zero_vec")]
    pub vector: [f32; EMBED_DIM],
    pub top_tags: Vec<TagAffinity>,
    pub anchor_games: Vec<AnchorGame>,
    pub anti_clusters: Vec<AntiCluster>,
    /// Games contributing meaningful signal (w >= 0.05).
    pub signal_count: u32,
    /// "low" | "medium" | "high"
    pub confidence: String,
    pub computed_at: u64,
}

// ---------------------------------------------------------------------------
// Weights & bounce detection
// ---------------------------------------------------------------------------

/// w(g) = base * status * recency * current  (see plan for rationale)
pub fn game_weight(sig: &GameSignal, now: u64) -> f64 {
    let base = (1.0 + sig.hours).ln() / (1.0 + 64.0f64).ln();
    let status = match sig.category {
        Category::NotAGame => 0.0,
        Category::Completed => 1.5,
        _ => 1.0 + 0.3 * (sig.ach_pct.unwrap_or(0.0) / 100.0).min(1.0),
    };
    let recency = if sig.rtime_last_played == 0 {
        0.75
    } else {
        let years = (now.saturating_sub(sig.rtime_last_played)) as f64 / SECS_PER_YEAR;
        0.5 + 0.5 * (-years / 2.0).exp()
    };
    let current = if sig.hours_2weeks > 0.0 { 1.25 } else { 1.0 };
    base * status * recency * current
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BounceKind {
    None,
    Bounced,
    Abandoned,
}

pub fn detect_bounce(sig: &GameSignal, now: u64) -> BounceKind {
    if sig.category == Category::Completed || sig.category == Category::NotAGame {
        return BounceKind::None;
    }
    let days_since = if sig.rtime_last_played == 0 {
        // Never-played games are neutral: you can't bounce off what you never launched
        return BounceKind::None;
    } else {
        now.saturating_sub(sig.rtime_last_played) / SECS_PER_DAY
    };

    // Short game actually finished (playtime covers HLTB main) is not a bounce
    let finished_short = sig
        .hltb_main_hours
        .map(|m| m <= sig.hours + 1.0)
        .unwrap_or(false);

    if (0.2..=2.0).contains(&sig.hours)
        && days_since > 180
        && sig.hours_2weeks == 0.0
        && !finished_short
    {
        return BounceKind::Bounced;
    }
    if sig.hours > 2.0
        && sig.hours <= 10.0
        && days_since > 365
        && sig
            .hltb_main_hours
            .map(|m| sig.hours < 0.25 * m)
            .unwrap_or(false)
    {
        return BounceKind::Abandoned;
    }
    BounceKind::None
}

// ---------------------------------------------------------------------------
// Profile computation
// ---------------------------------------------------------------------------

fn l2_normalize(v: &mut [f32; EMBED_DIM]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

pub fn dot(a: &[f32; EMBED_DIM], b: &[f32; EMBED_DIM]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

pub fn compute_profile(signals: &[GameSignal], now: u64) -> TasteProfile {
    let mut weights: HashMap<u64, f64> = HashMap::new();
    let mut bounce_kinds: HashMap<u64, BounceKind> = HashMap::new();
    for sig in signals {
        weights.insert(sig.appid, game_weight(sig, now));
        bounce_kinds.insert(sig.appid, detect_bounce(sig, now));
    }

    // Taste vector: weighted sum over non-bounced games with vectors
    let mut t = [0.0f32; EMBED_DIM];
    for sig in signals {
        let kind = bounce_kinds[&sig.appid];
        if kind != BounceKind::None {
            continue;
        }
        let w = weights[&sig.appid];
        if w <= 0.0 {
            continue;
        }
        if let Some(v) = &sig.vector {
            for (acc, x) in t.iter_mut().zip(v.iter()) {
                *acc += w as f32 * x;
            }
        }
    }
    l2_normalize(&mut t);

    let signal_count = signals
        .iter()
        .filter(|s| weights[&s.appid] >= 0.05)
        .count() as u32;
    let confidence = if signal_count < 8 {
        "low"
    } else if signal_count < 20 {
        "medium"
    } else {
        "high"
    };

    // Tag affinities: sum w(g) per tag over non-bounced games
    let mut tag_weight: HashMap<&str, f64> = HashMap::new();
    let mut tag_examples: HashMap<&str, Vec<(f64, u64)>> = HashMap::new();
    for sig in signals {
        if bounce_kinds[&sig.appid] != BounceKind::None {
            continue;
        }
        let w = weights[&sig.appid];
        if w < 0.05 {
            continue;
        }
        for tag in sig.tags.iter().take(10) {
            *tag_weight.entry(tag.as_str()).or_default() += w;
            tag_examples
                .entry(tag.as_str())
                .or_default()
                .push((w, sig.appid));
        }
    }
    let max_tag_w = tag_weight.values().cloned().fold(0.0f64, f64::max).max(1e-9);
    let mut top_tags: Vec<TagAffinity> = tag_weight
        .iter()
        .map(|(tag, w)| {
            let mut ex = tag_examples[tag].clone();
            ex.sort_by(|a, b| b.0.total_cmp(&a.0));
            TagAffinity {
                tag: tag.to_string(),
                weight: w / max_tag_w,
                example_appids: ex.into_iter().take(3).map(|(_, a)| a).collect(),
            }
        })
        .collect();
    top_tags.sort_by(|a, b| b.weight.total_cmp(&a.weight));
    top_tags.truncate(12);

    // Anchors: top-10 by weight (non-bounced, with vectors)
    let mut anchors: Vec<AnchorGame> = signals
        .iter()
        .filter(|s| bounce_kinds[&s.appid] == BounceKind::None && s.vector.is_some())
        .map(|s| AnchorGame {
            appid: s.appid,
            name: s.name.clone(),
            weight: weights[&s.appid],
            vector: s.vector,
        })
        .collect();
    anchors.sort_by(|a, b| b.weight.total_cmp(&a.weight));
    anchors.truncate(10);

    let anti_clusters = build_anti_clusters(signals, &weights, &bounce_kinds);

    TasteProfile {
        vector: t,
        top_tags,
        anchor_games: anchors,
        anti_clusters,
        signal_count,
        confidence: confidence.to_string(),
        computed_at: now,
    }
}

fn build_anti_clusters(
    signals: &[GameSignal],
    weights: &HashMap<u64, f64>,
    bounce_kinds: &HashMap<u64, BounceKind>,
) -> Vec<AntiCluster> {
    // n(t): 1.0 per bounce, 0.5 per abandon, over each bounced game's top-5 tags
    let mut tag_n: HashMap<&str, f64> = HashMap::new();
    let mut tag_games: HashMap<&str, HashSet<u64>> = HashMap::new();
    let bounced_sigs: Vec<(&GameSignal, BounceKind)> = signals
        .iter()
        .filter_map(|s| match bounce_kinds[&s.appid] {
            BounceKind::None => None,
            k => Some((s, k)),
        })
        .collect();

    for (sig, kind) in &bounced_sigs {
        let contribution = match kind {
            BounceKind::Bounced => 1.0,
            BounceKind::Abandoned => 0.5,
            BounceKind::None => unreachable!(),
        };
        for tag in sig.tags.iter().take(5) {
            *tag_n.entry(tag.as_str()).or_default() += contribution;
            tag_games.entry(tag.as_str()).or_default().insert(sig.appid);
        }
    }

    // engaged(t): owned games with tag t and w >= 0.3 (not bounced)
    let mut engaged: HashMap<&str, u32> = HashMap::new();
    for sig in signals {
        if bounce_kinds[&sig.appid] != BounceKind::None {
            continue;
        }
        if weights[&sig.appid] < 0.3 {
            continue;
        }
        for tag in sig.tags.iter().take(10) {
            *engaged.entry(tag.as_str()).or_default() += 1;
        }
    }

    // Anti-tags: n >= 2 and dominance ratio >= 0.6
    let mut anti_tags: Vec<(&str, f64)> = tag_n
        .iter()
        .filter(|(tag, n)| {
            let e = *engaged.get(**tag).unwrap_or(&0) as f64;
            **n >= 2.0 && **n / (**n + e) >= 0.6
        })
        .map(|(t, n)| (*t, *n))
        .collect();
    anti_tags.sort_by(|a, b| b.1.total_cmp(&a.1));

    // Merge anti-tags whose bounced-game sets overlap >= 50% into clusters
    let mut clusters: Vec<(Vec<&str>, HashSet<u64>, f64)> = Vec::new();
    for (tag, n) in &anti_tags {
        let games = &tag_games[tag];
        let mut merged = false;
        for (ctags, cgames, cn) in clusters.iter_mut() {
            let inter = games.intersection(cgames).count() as f64;
            let smaller = games.len().min(cgames.len()).max(1) as f64;
            if inter / smaller >= 0.5 {
                ctags.push(tag);
                cgames.extend(games.iter());
                *cn = cn.max(*n);
                merged = true;
                break;
            }
        }
        if !merged {
            clusters.push((vec![tag], games.clone(), *n));
        }
    }

    let by_appid: HashMap<u64, &GameSignal> = signals.iter().map(|s| (s.appid, s)).collect();
    clusters
        .into_iter()
        .map(|(tags, game_set, _)| {
            // Recompute effective n for the merged set
            let mut n_cluster: f64 = 0.0;
            let mut bounced: Vec<BouncedGame> = Vec::new();
            let mut vec_sum = [0.0f32; EMBED_DIM];
            for appid in &game_set {
                let sig = by_appid[appid];
                let kind = bounce_kinds[appid];
                n_cluster += match kind {
                    BounceKind::Bounced => 1.0,
                    BounceKind::Abandoned => 0.5,
                    BounceKind::None => 0.0,
                };
                bounced.push(BouncedGame {
                    appid: sig.appid,
                    name: sig.name.clone(),
                    playtime_hours: sig.hours,
                    last_played: sig.rtime_last_played,
                    kind: match kind {
                        BounceKind::Abandoned => "abandoned".into(),
                        _ => "bounced".into(),
                    },
                });
                if let Some(v) = &sig.vector {
                    for (acc, x) in vec_sum.iter_mut().zip(v.iter()) {
                        *acc += x;
                    }
                }
            }
            l2_normalize(&mut vec_sum);
            bounced.sort_by(|a, b| a.name.cmp(&b.name));
            AntiCluster {
                label: tags[0].to_string(),
                tags: tags.iter().map(|t| t.to_string()).collect(),
                bounced,
                strength: (n_cluster / 6.0).min(1.0),
                vector: vec_sum,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Candidate scoring (Discover / wishlist)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredCandidate {
    pub score: f64,
    pub sim: f64,
    pub quality: f64,
    pub warning: Option<String>,
}

pub fn score_candidate(
    profile: &TasteProfile,
    candidate_vec: &[f32; EMBED_DIM],
    review_positive_pct: u8,
    review_total: u32,
) -> ScoredCandidate {
    let sim = (dot(&profile.vector, candidate_vec) as f64).clamp(0.0, 1.0);
    let positive = review_total as f64 * review_positive_pct as f64 / 100.0;
    let quality = (positive + 10.0) / (review_total as f64 + 20.0);

    let mut penalty = 0.0f64;
    let mut warning: Option<String> = None;
    for cluster in &profile.anti_clusters {
        let d = dot(&cluster.vector, candidate_vec) as f64;
        let p = cluster.strength * 0.35 * ((d - 0.45).max(0.0) / 0.55);
        if p > penalty {
            penalty = p;
        }
        if d >= 0.55 && cluster.strength >= 0.5 && warning.is_none() {
            let n = cluster.bounced.len();
            warning = Some(format!(
                "You've bounced off {n} {} game{} — this looks like one",
                cluster.label,
                if n == 1 { "" } else { "s" }
            ));
        }
    }

    let (ws, wq) = if profile.signal_count < 15 {
        (0.50, 0.50)
    } else {
        (0.75, 0.25)
    };
    ScoredCandidate {
        score: ws * sim + wq * quality - penalty,
        sim,
        quality,
        warning,
    }
}

/// Deterministic reason string: top-2 anchors by dot (among anchors with
/// w >= 0.3) plus tag intersection with the user's top tags.
pub fn reason_for(
    profile: &TasteProfile,
    candidate_vec: &[f32; EMBED_DIM],
    candidate_tags: &[String],
) -> String {
    let mut scored_anchors: Vec<(&AnchorGame, f32)> = profile
        .anchor_games
        .iter()
        .filter(|a| a.weight >= 0.3)
        .filter_map(|a| a.vector.as_ref().map(|v| (a, dot(v, candidate_vec))))
        .collect();
    scored_anchors.sort_by(|a, b| b.1.total_cmp(&a.1));

    let anchor_part = match scored_anchors.as_slice() {
        [] => String::new(),
        [(a, _)] => format!("Because you played {}", a.name),
        [(a, _), (b, _), ..] => format!("Because you played {} and {}", a.name, b.name),
    };

    let user_tags: HashSet<&str> = profile.top_tags.iter().map(|t| t.tag.as_str()).collect();
    let shared: Vec<&str> = candidate_tags
        .iter()
        .take(8)
        .map(|t| t.as_str())
        .filter(|t| user_tags.contains(t))
        .take(2)
        .collect();

    match (anchor_part.is_empty(), shared.is_empty()) {
        (false, false) => format!("{anchor_part} · {}", shared.join(", ")),
        (false, true) => anchor_part,
        (true, false) => shared.join(", "),
        (true, true) => "Matches your library's overall profile".to_string(),
    }
}

// ---------------------------------------------------------------------------
// "More like this"
// ---------------------------------------------------------------------------

/// Lowercase, strip ™®©, cut subtitle after ":" or "–".
pub fn normalize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !matches!(c, '™' | '®' | '©'))
        .collect();
    let cut = cleaned
        .split([':', '–'])
        .next()
        .unwrap_or(&cleaned)
        .trim()
        .to_lowercase();
    cut
}

fn is_stopword(t: &str) -> bool {
    matches!(t, "the" | "of" | "a" | "an" | "and")
}

fn is_numeric_ish(t: &str) -> bool {
    !t.is_empty()
        && (t.chars().all(|c| c.is_ascii_digit())
            || t.chars().all(|c| matches!(c, 'i' | 'v' | 'x' | 'l' | 'c' | 'd' | 'm')))
}

/// Same franchise: one normalized name prefixes the other, or the first two
/// MEANINGFUL tokens match (stopwords skipped — otherwise "Slay the Spire" and
/// "Slay the Princess" both reduce to "slay the" and false-positive). Numeric
/// second tokens ("3" vs "2") still count as the same franchise.
pub fn same_franchise(a: &str, b: &str) -> bool {
    let na = normalize_name(a);
    let nb = normalize_name(b);
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    if na.starts_with(&nb) || nb.starts_with(&na) {
        return true;
    }
    let ta: Vec<&str> = na.split_whitespace().filter(|t| !is_stopword(t)).take(2).collect();
    let tb: Vec<&str> = nb.split_whitespace().filter(|t| !is_stopword(t)).take(2).collect();
    if ta.len() < 2 || tb.len() < 2 || ta[0] != tb[0] {
        return false;
    }
    ta[1] == tb[1] || (is_numeric_ish(ta[1]) && is_numeric_ish(tb[1]))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarGame {
    pub appid: u32,
    pub name: String,
    pub similarity: f64,
    pub tags: Vec<String>,
    pub review_positive_pct: u8,
    pub review_total: u32,
    /// Cross-genre find (primary tag differs from the source game's).
    pub non_obvious: bool,
    pub owned: bool,
}

/// Nearest neighbors excluding franchise/developer near-duplicates, diversified
/// with MMR (λ = 0.75).
pub fn similar_games(
    catalog: &Catalog,
    source_vec: &[f32; EMBED_DIM],
    source_meta: &CatalogGameMeta,
    owned_appids: &HashSet<u32>,
    k: usize,
) -> Vec<SimilarGame> {
    let source_dev: HashSet<&str> = source_meta.developers.iter().map(|d| d.as_str()).collect();
    let source_lead = normalize_name(&source_meta.name)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();

    let pool = catalog.top_matches(source_vec, 200, |_, m| m.appid != source_meta.appid);

    // Drop franchise + same-dev-same-leading-token near-duplicates
    let filtered: Vec<(u32, f32)> = pool
        .into_iter()
        .filter(|(row, _)| {
            let m = &catalog.meta[*row as usize];
            if same_franchise(&m.name, &source_meta.name) {
                return false;
            }
            let shares_dev = m.developers.iter().any(|d| source_dev.contains(d.as_str()));
            let lead = normalize_name(&m.name);
            let lead = lead.split_whitespace().next().unwrap_or_default();
            !(shares_dev && !source_lead.is_empty() && lead == source_lead)
        })
        .collect();

    // MMR diversification
    let lambda = 0.75f32;
    let mut selected: Vec<(u32, f32)> = Vec::new();
    let mut remaining = filtered;
    while selected.len() < k && !remaining.is_empty() {
        let mut best_idx = 0;
        let mut best_score = f32::NEG_INFINITY;
        for (i, (row, sim_src)) in remaining.iter().enumerate() {
            let max_sel: f32 = selected
                .iter()
                .map(|(srow, _)| {
                    let a = catalog.vector_f32(*row);
                    let b = catalog.vector_f32(*srow);
                    dot(&a, &b)
                })
                .fold(0.0, f32::max);
            let mmr = lambda * sim_src - (1.0 - lambda) * max_sel;
            if mmr > best_score {
                best_score = mmr;
                best_idx = i;
            }
        }
        selected.push(remaining.swap_remove(best_idx));
    }

    let source_primary = source_meta.tags.first().map(|t| t.as_str()).unwrap_or("");
    selected
        .into_iter()
        .map(|(row, sim)| {
            let m = &catalog.meta[row as usize];
            SimilarGame {
                appid: m.appid,
                name: m.name.clone(),
                similarity: sim as f64,
                tags: m.tags.iter().take(5).cloned().collect(),
                review_positive_pct: m.review_positive_pct,
                review_total: m.review_total,
                non_obvious: m.tags.first().map(|t| t.as_str()).unwrap_or("") != source_primary,
                owned: owned_appids.contains(&m.appid),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_756_684_800; // 2025-09-01-ish; exact value irrelevant

    fn sig(appid: u64, hours: f64, category: Category) -> GameSignal {
        GameSignal {
            appid,
            name: format!("Game {appid}"),
            hours,
            hours_2weeks: 0.0,
            rtime_last_played: NOW - 30 * SECS_PER_DAY,
            ach_pct: None,
            category,
            hltb_main_hours: None,
            vector: Some(unit_vec(appid as usize % EMBED_DIM)),
            tags: vec!["Action".into()],
        }
    }

    fn unit_vec(axis: usize) -> [f32; EMBED_DIM] {
        let mut v = [0.0f32; EMBED_DIM];
        v[axis] = 1.0;
        v
    }

    #[test]
    fn weight_formula_known_values() {
        // base: 0h → 0; 64h → 1.0. status: completed ×1.5. recency 30d ≈ ~0.98.
        let zero = sig(1, 0.0, Category::InProgress);
        assert_eq!(game_weight(&zero, NOW), 0.0);

        let mut s = sig(2, 64.0, Category::InProgress);
        s.rtime_last_played = NOW; // no decay
        let w = game_weight(&s, NOW);
        assert!((w - 1.0).abs() < 1e-9, "64h fresh in-progress = 1.0, got {w}");

        let mut c = sig(3, 64.0, Category::Completed);
        c.rtime_last_played = NOW;
        assert!((game_weight(&c, NOW) - 1.5).abs() < 1e-9);

        // achievement tilt: 100% ach → ×1.3
        let mut a = sig(4, 64.0, Category::InProgress);
        a.rtime_last_played = NOW;
        a.ach_pct = Some(100.0);
        assert!((game_weight(&a, NOW) - 1.3).abs() < 1e-9);

        // currently playing multiplier
        let mut p = sig(5, 64.0, Category::InProgress);
        p.rtime_last_played = NOW;
        p.hours_2weeks = 3.0;
        assert!((game_weight(&p, NOW) - 1.25).abs() < 1e-9);

        // recency floor: decades-old play still counts half
        let mut old = sig(6, 64.0, Category::InProgress);
        old.rtime_last_played = NOW - 40 * 365 * SECS_PER_DAY;
        let w_old = game_weight(&old, NOW);
        assert!(w_old > 0.49 && w_old < 0.55, "floor ~0.5, got {w_old}");

        // NOT_A_GAME contributes nothing
        assert_eq!(game_weight(&sig(7, 100.0, Category::NotAGame), NOW), 0.0);
    }

    #[test]
    fn bounce_detection_boundaries() {
        let old = NOW - 200 * SECS_PER_DAY;

        let mut s = sig(1, 0.19, Category::InProgress);
        s.rtime_last_played = old;
        assert_eq!(detect_bounce(&s, NOW), BounceKind::None, "0.19h = never really launched");

        s.hours = 0.2;
        assert_eq!(detect_bounce(&s, NOW), BounceKind::Bounced, "0.2h boundary");

        s.hours = 2.0;
        assert_eq!(detect_bounce(&s, NOW), BounceKind::Bounced, "2.0h boundary");

        s.hours = 2.1;
        assert_eq!(detect_bounce(&s, NOW), BounceKind::None, "2.1h not bounced (and no hltb → not abandoned)");

        // hltb exemption: 1.5h played, hltb main 2h → finished a short game
        let mut short = sig(2, 1.5, Category::InProgress);
        short.rtime_last_played = old;
        short.hltb_main_hours = Some(2.0);
        assert_eq!(detect_bounce(&short, NOW), BounceKind::None, "finished short game");

        short.hltb_main_hours = Some(10.0);
        assert_eq!(detect_bounce(&short, NOW), BounceKind::Bounced, "1.5h into a 10h game");

        // recently played → not a bounce
        let mut recent = sig(3, 1.0, Category::InProgress);
        recent.rtime_last_played = NOW - 30 * SECS_PER_DAY;
        assert_eq!(detect_bounce(&recent, NOW), BounceKind::None);

        // abandoned: 5h into a 40h game, untouched for >1y
        let mut ab = sig(4, 5.0, Category::InProgress);
        ab.rtime_last_played = NOW - 400 * SECS_PER_DAY;
        ab.hltb_main_hours = Some(40.0);
        assert_eq!(detect_bounce(&ab, NOW), BounceKind::Abandoned);

        // completed games never bounce
        let mut done = sig(5, 1.0, Category::Completed);
        done.rtime_last_played = old;
        assert_eq!(detect_bounce(&done, NOW), BounceKind::None);
    }

    #[test]
    fn anti_cluster_engagement_normalization() {
        let old = NOW - 200 * SECS_PER_DAY;
        let mut signals: Vec<GameSignal> = Vec::new();

        // 3 bounced FPS games
        for i in 0..3 {
            let mut s = sig(100 + i, 1.0, Category::InProgress);
            s.rtime_last_played = old;
            s.tags = vec!["FPS".into(), "Shooter".into()];
            s.vector = Some(unit_vec(0));
            signals.push(s);
        }

        // Case A: no engaged FPS games → cluster forms
        let profile = compute_profile(&signals, NOW);
        assert_eq!(profile.anti_clusters.len(), 1, "one merged FPS cluster");
        assert_eq!(profile.anti_clusters[0].bounced.len(), 3);
        assert!((profile.anti_clusters[0].strength - 0.5).abs() < 1e-9, "3 bounces / 6 = 0.5");

        // Case B: user also finished 30 FPS games → no anti-cluster
        for i in 0..30 {
            let mut s = sig(200 + i, 64.0, Category::Completed);
            s.rtime_last_played = NOW;
            s.tags = vec!["FPS".into(), "Shooter".into()];
            s.vector = Some(unit_vec(1));
            signals.push(s);
        }
        let profile_b = compute_profile(&signals, NOW);
        assert!(
            profile_b.anti_clusters.is_empty(),
            "engagement normalization must kill the FPS anti-cluster"
        );
    }

    #[test]
    fn tiny_library_blend() {
        let mut profile = compute_profile(&[sig(1, 64.0, Category::InProgress)], NOW);
        assert_eq!(profile.confidence, "low");
        assert!(profile.signal_count < 15);

        let v = unit_vec(1 % EMBED_DIM);
        // sim = dot(T, v). T is unit_vec(1) here (single game), so sim = 1.
        let scored_small = score_candidate(&profile, &v, 80, 1000);
        // small library: 0.5*sim + 0.5*quality
        let q = (800.0 + 10.0) / 1020.0;
        assert!((scored_small.score - (0.5 + 0.5 * q)).abs() < 1e-6);

        // fake a big library
        profile.signal_count = 30;
        let scored_big = score_candidate(&profile, &v, 80, 1000);
        assert!((scored_big.score - (0.75 + 0.25 * q)).abs() < 1e-6);
    }

    #[test]
    fn warning_emitted_for_anti_cluster_match() {
        let old = NOW - 200 * SECS_PER_DAY;
        let mut signals: Vec<GameSignal> = Vec::new();
        for i in 0..4 {
            let mut s = sig(100 + i, 1.0, Category::InProgress);
            s.rtime_last_played = old;
            s.tags = vec!["Competitive".into()];
            s.vector = Some(unit_vec(7));
            signals.push(s);
        }
        // one loved game elsewhere so T isn't the anti direction
        let mut loved = sig(500, 64.0, Category::Completed);
        loved.vector = Some(unit_vec(9));
        loved.tags = vec!["RPG".into()];
        signals.push(loved);

        let profile = compute_profile(&signals, NOW);
        assert_eq!(profile.anti_clusters.len(), 1);
        assert!(profile.anti_clusters[0].strength >= 0.5);

        // candidate aligned with the anti-cluster direction
        let scored = score_candidate(&profile, &unit_vec(7), 90, 5000);
        assert!(scored.warning.is_some(), "expected bounce warning");
        assert!(scored.warning.unwrap().contains("bounced off 4"));

        // orthogonal candidate: no warning, no penalty
        let clean = score_candidate(&profile, &unit_vec(9), 90, 5000);
        assert!(clean.warning.is_none());
    }

    #[test]
    fn franchise_filter() {
        assert!(same_franchise("The Witcher® 3: Wild Hunt", "The Witcher 2"));
        assert!(same_franchise("DARK SOULS™ III", "DARK SOULS™: REMASTERED"));
        assert!(same_franchise("Half-Life 2", "Half-Life"));
        assert!(!same_franchise("The Witcher 3", "The Elder Scrolls V"));
        assert!(!same_franchise("Hades", "Dead Cells"));
        assert!(!same_franchise("Portal 2", "Celeste"));
        // Stopword-blind compare must not merge unrelated "X the Y" titles
        assert!(!same_franchise("Slay the Spire", "Slay the Princess"));
        assert!(!same_franchise("Rise of Industry", "Rise of Nations"));
        // But real franchises with shared meaningful tokens still match
        assert!(same_franchise("Assassin's Creed Origins", "Assassin's Creed Odyssey"));
        assert!(same_franchise("Far Cry 3", "Far Cry Primal"));
    }

    #[test]
    fn normalize_name_variants() {
        assert_eq!(normalize_name("The Witcher® 3: Wild Hunt"), "the witcher 3");
        assert_eq!(normalize_name("Sekiro™: Shadows Die Twice"), "sekiro");
        assert_eq!(normalize_name("Divinity – Original Sin"), "divinity");
    }

    fn load_mini_catalog() -> Catalog {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/catalog_mini.gkc");
        Catalog::load(&path).expect("mini fixture")
    }

    #[test]
    fn similar_games_excludes_self_and_marks_owned() {
        let cat = load_mini_catalog();
        let (row, meta) = cat.get(1145360).expect("Hades in fixture"); // Hades
        let vec = cat.vector_f32(row);
        let meta = meta.clone();
        let owned: HashSet<u32> = [588650u32].into_iter().collect(); // own Dead Cells

        let sims = similar_games(&cat, &vec, &meta, &owned, 5);
        assert!(!sims.is_empty() && sims.len() <= 5);
        assert!(sims.iter().all(|s| s.appid != 1145360), "never recommends itself");
        let dead_cells = sims.iter().find(|s| s.appid == 588650);
        if let Some(dc) = dead_cells {
            assert!(dc.owned, "owned games must be flagged");
        }
        // Results ordered by descending similarity within MMR's selection
        assert!(sims[0].similarity > 0.0);
    }

    #[test]
    fn reason_for_names_anchors_and_shared_tags() {
        let mut anchor_vec = unit_vec(3);
        anchor_vec[4] = 0.4;
        let profile = TasteProfile {
            vector: unit_vec(3),
            top_tags: vec![TagAffinity {
                tag: "Roguelite".into(),
                weight: 1.0,
                example_appids: vec![1],
            }],
            anchor_games: vec![AnchorGame {
                appid: 1,
                name: "Hades".into(),
                weight: 1.2,
                vector: Some(anchor_vec),
            }],
            anti_clusters: vec![],
            signal_count: 20,
            confidence: "high".into(),
            computed_at: NOW,
        };
        let candidate = unit_vec(3);
        let reason = reason_for(&profile, &candidate, &["Roguelite".into(), "Indie".into()]);
        assert!(reason.contains("Hades"), "reason must cite the anchor: {reason}");
        assert!(reason.contains("Roguelite"), "reason must cite shared tag: {reason}");

        // No anchors above weight threshold + no shared tags → generic fallback
        let empty_profile = TasteProfile {
            vector: unit_vec(0),
            top_tags: vec![],
            anchor_games: vec![],
            anti_clusters: vec![],
            signal_count: 3,
            confidence: "low".into(),
            computed_at: NOW,
        };
        let generic = reason_for(&empty_profile, &candidate, &["Puzzle".into()]);
        assert_eq!(generic, "Matches your library's overall profile");
    }
}
