//! Dev sanity check: run the taste engine against the REAL local caches and
//! print the profile + top discover picks. Not a test — output needs eyeballs.
//!
//!     cargo run --example taste_sanity

use gamekeeper_lib::{build_game_signals, cache, catalog::Catalog, config, embed, taste};
use std::collections::HashSet;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cat = Catalog::load(&manifest.join("resources/catalog.gkc")).expect("catalog");
    println!(
        "Catalog: {} games, dataset {}",
        cat.header.game_count, cat.header.dataset_date
    );

    let embedder = embed::Embedder::load(&manifest.join("resources/potion-base-8M")).ok();

    let cfg = config::load_config().expect("config (run the app once first)");
    let games = cache::load_library_cache_any_age(&cfg.steam_id).expect("library cache");
    let classifications: Vec<_> = cache::load_saved_classifications().into_values().collect();
    let store_cache = cache::load_store_cache();
    let hltb = cache::load_hltb_cache();
    println!(
        "Library: {} games, {} classifications, {} store entries, {} hltb",
        games.len(),
        classifications.len(),
        store_cache.len(),
        hltb.len()
    );

    let signals = build_game_signals(
        &games,
        &classifications,
        &store_cache,
        &hltb,
        &cat,
        embedder.as_ref(),
    );
    let in_catalog = signals.iter().filter(|s| s.vector.is_some()).count();
    println!("Signals: {} total, {} with vectors", signals.len(), in_catalog);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let t0 = std::time::Instant::now();
    let profile = taste::compute_profile(&signals, now);
    println!(
        "\n=== TASTE PROFILE ({} signals, {} confidence, computed in {:?}) ===",
        profile.signal_count,
        profile.confidence,
        t0.elapsed()
    );
    println!("\nTop tags:");
    for t in &profile.top_tags {
        println!("  {:<24} {:.2}", t.tag, t.weight);
    }
    println!("\nAnchor games:");
    for a in &profile.anchor_games {
        println!("  {:<40} w={:.2}", a.name, a.weight);
    }
    println!("\nAnti-clusters:");
    if profile.anti_clusters.is_empty() {
        println!("  (none)");
    }
    for c in &profile.anti_clusters {
        let names: Vec<&str> = c.bounced.iter().map(|b| b.name.as_str()).collect();
        println!(
            "  {} (strength {:.2}, tags {:?}): {}",
            c.label,
            c.strength,
            c.tags,
            names.join(", ")
        );
    }

    let owned: HashSet<u32> = games.iter().map(|g| g.appid as u32).collect();
    let t1 = std::time::Instant::now();
    let pool = cat.top_matches(&profile.vector, 300, |_, m| {
        !owned.contains(&m.appid) && !m.adult && m.review_positive_pct >= 70 && m.review_total >= 50
    });
    let mut scored: Vec<(f64, String, String, Option<String>)> = pool
        .into_iter()
        .map(|(row, _)| {
            let m = &cat.meta[row as usize];
            let v = cat.vector_f32(row);
            let s = taste::score_candidate(&profile, &v, m.review_positive_pct, m.review_total);
            (
                s.score,
                m.name.clone(),
                taste::reason_for(&profile, &v, &m.tags),
                s.warning,
            )
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("\n=== TOP 15 DISCOVER PICKS (scan+score in {:?}) ===", t1.elapsed());
    for (score, name, reason, warning) in scored.iter().take(15) {
        println!("  {score:.3}  {name}");
        println!("         {reason}");
        if let Some(w) = warning {
            println!("         ⚠ {w}");
        }
    }

    // More-like-this spot check on the highest-playtime game in catalog
    if let Some(top) = signals
        .iter()
        .filter(|s| s.vector.is_some())
        .max_by(|a, b| a.hours.total_cmp(&b.hours))
    {
        if let Some((row, meta)) = cat.get(top.appid as u32) {
            let v = cat.vector_f32(row);
            let sims = taste::similar_games(&cat, &v, &meta.clone(), &owned, 8);
            println!("\n=== MORE LIKE \"{}\" ===", meta.name);
            for s in sims {
                println!(
                    "  {:.3}  {:<40} {}{}",
                    s.similarity,
                    s.name,
                    if s.owned { "[owned] " } else { "" },
                    if s.non_obvious { "[unexpected]" } else { "" }
                );
            }
        }
    }
}
