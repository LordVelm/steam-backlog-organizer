use crate::steam_api::{OwnedGame, StoreDetails};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Category {
    #[serde(rename = "COMPLETED")]
    Completed,
    #[serde(rename = "IN_PROGRESS")]
    InProgress,
    #[serde(rename = "ENDLESS")]
    Endless,
    #[serde(rename = "NOT_A_GAME")]
    NotAGame,
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Category::Completed => write!(f, "COMPLETED"),
            Category::InProgress => write!(f, "IN_PROGRESS"),
            Category::Endless => write!(f, "ENDLESS"),
            Category::NotAGame => write!(f, "NOT_A_GAME"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Classification {
    pub appid: u64,
    pub name: String,
    pub category: Category,
    pub confidence: String,
    pub reason: String,
}

// -- Regex patterns (compiled once) --

static NOT_A_GAME_NAME_PATTERNS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(dedicated server|soundtrack|ost\b|sdk\b|benchmark|demo\b|teaser|playable teaser|tech demo|modding tool|level editor|map editor|wallpaper engine|rpg maker|game maker|vr home|steamvr|test server|public test)\b"
    ).unwrap()
});

static COMPLETION_ACHIEVEMENT_PATTERNS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(final.?boss|last.?boss|beat.?the.?game|the.?end|credits|end.?credits|complete.?the.?game|finish.?the.?game|game.?complete|chapter.?\d+.?complete|act.?\d+.?complete|epilogue|finale|platinum|true.?ending|good.?ending|bad.?ending|beat.?campaign|campaign.?complete|story.?complete)"
    ).unwrap()
});

/// Classify a single game using rule-based logic.
/// Returns (category, reason). Always returns a classification.
///
/// Priority order matches Python exactly:
///   1. Steam type = demo/tool/music/dlc → NOT_A_GAME
///   2. Name patterns (dedicated server, soundtrack, SDK, etc.) → NOT_A_GAME
///   3. Story-completion achievements earned → COMPLETED
///   4. High achievement % (≥80%) → COMPLETED
///   5. Multiplayer-only (has MP, no SP) → ENDLESS
///   6. MMO genre → ENDLESS
///   7. Sandbox/strategy/simulation genres (no SP) → ENDLESS
///   8. Moderate achievements + playtime → COMPLETED
///   9. Significant playtime in SP → COMPLETED
///   10. Sandbox/strategy/simulation (even with SP, no achievements) → ENDLESS
///   11. Unplayed single-player → IN_PROGRESS (backlog)
///   12. Unplayed, no store info → IN_PROGRESS
///   13. Low playtime in SP → IN_PROGRESS
///   14. Fallback → IN_PROGRESS
pub fn classify_by_rules(game: &OwnedGame, store_info: Option<&StoreDetails>) -> (Category, String) {
    let name = &game.name;
    let playtime = game.playtime_hours;

    // Store info
    let (store_type, genres, categories) = if let Some(info) = store_info {
        let st = info.app_type.to_lowercase();
        let g: Vec<String> = info.genres.iter().map(|s| s.to_lowercase()).collect();
        let c: Vec<String> = info.categories.iter().map(|s| s.to_lowercase()).collect();
        (st, g, c)
    } else {
        (String::new(), Vec::new(), Vec::new())
    };

    // Rule 1: Steam type indicates non-game
    let non_game_types = ["demo", "tool", "music", "dlc", "video", "hardware", "mod"];
    if non_game_types.contains(&store_type.as_str()) {
        return (Category::NotAGame, format!("Steam type: {store_type}"));
    }

    // Rule 2: Name patterns
    if let Some(m) = NOT_A_GAME_NAME_PATTERNS.find(name) {
        return (
            Category::NotAGame,
            format!("Name pattern: {}", m.as_str()),
        );
    }

    // Rule 3: Story-completion achievements earned
    if let Some(ref ach) = game.achievements {
        for ach_name in &ach.names_achieved {
            if COMPLETION_ACHIEVEMENT_PATTERNS.is_match(ach_name) {
                return (
                    Category::Completed,
                    format!("Story achievement: {ach_name}"),
                );
            }
        }

        // Rule 4: High achievement percentage
        if ach.percentage >= 80.0 {
            return (
                Category::Completed,
                format!("Achievement completion: {}%", ach.percentage),
            );
        }
    }

    // Rule 5: Multiplayer-only
    let has_mp = categories
        .iter()
        .any(|c| c.contains("multi-player") || c.contains("multiplayer"));
    let has_sp = categories.iter().any(|c| c.contains("single-player"));

    if has_mp && !has_sp {
        return (
            Category::Endless,
            "Multiplayer-only (no single-player)".into(),
        );
    }

    // Rule 6: MMO genre
    if genres.iter().any(|g| g.contains("mmo")) {
        return (Category::Endless, "MMO genre".into());
    }

    // Rule 7: Sandbox/strategy/simulation genres with no SP
    let endless_genres: HashSet<&str> =
        ["simulation", "strategy", "casual", "sports", "racing"]
            .into_iter()
            .collect();

    if !genres.is_empty() && !has_sp {
        let game_genres: HashSet<&str> = genres.iter().map(|s| s.as_str()).collect();
        let matched: Vec<&str> = game_genres.intersection(&endless_genres).copied().collect();
        if !matched.is_empty() {
            let mut matched_sorted = matched.clone();
            matched_sorted.sort();
            return (
                Category::Endless,
                format!("Genre: {}", matched_sorted.join(", ")),
            );
        }
    }

    // Rule 8: Achievement % with significant playtime suggests completion
    // Tiered: lower achievement % accepted with more playtime (open-world games
    // have tons of side content that dilutes % even after beating the main story)
    if let Some(ref ach) = game.achievements {
        if ach.percentage >= 40.0 && playtime >= 5.0 {
            return (
                Category::Completed,
                format!(
                    "Likely completed ({}% achievements, {:.1}h played)",
                    ach.percentage, playtime
                ),
            );
        }
        if ach.percentage >= 20.0 && playtime >= 20.0 && has_sp {
            return (
                Category::Completed,
                format!(
                    "Likely completed ({}% achievements, {:.1}h played, single-player)",
                    ach.percentage, playtime
                ),
            );
        }
    }

    // Rule 9: Significant playtime with single-player suggests completion
    if has_sp && playtime >= 15.0 && game.achievements.is_none() {
        return (
            Category::Completed,
            format!("Likely completed (single-player, {playtime:.1}h played)"),
        );
    }

    // Rule 9b: Very high playtime with no achievements → likely completed
    // Catches games where achievements aren't tracked on Steam (e.g. Ubisoft Connect)
    if has_sp && playtime >= 20.0 {
        if let Some(ref ach) = game.achievements {
            if ach.achieved == 0 && ach.total > 0 {
                return (
                    Category::Completed,
                    format!("Likely completed (single-player, {:.1}h played, achievements tracked externally)", playtime),
                );
            }
        }
    }

    // Rule 10: Sandbox/strategy/simulation genres (even with SP) → ENDLESS if no achievements
    if !genres.is_empty() && game.achievements.is_none() {
        let game_genres: HashSet<&str> = genres.iter().map(|s| s.as_str()).collect();
        let matched: Vec<&str> = game_genres.intersection(&endless_genres).copied().collect();
        if !matched.is_empty() {
            let mut matched_sorted = matched.clone();
            matched_sorted.sort();
            return (
                Category::Endless,
                format!("Genre: {}", matched_sorted.join(", ")),
            );
        }
    }

    // Rule 11: Unplayed with single-player → backlog
    if playtime == 0.0 && has_sp {
        return (
            Category::InProgress,
            "Unplayed single-player game (backlog)".into(),
        );
    }

    // Rule 12: Unplayed with no store info → default to IN_PROGRESS
    if playtime == 0.0 && store_info.is_none() {
        return (Category::InProgress, "Unplayed game (backlog)".into());
    }

    // Rule 13: Low playtime with single-player → still in progress
    if has_sp && playtime > 0.0 && playtime < 15.0 {
        return (
            Category::InProgress,
            format!("Single-player with low playtime ({playtime}h)"),
        );
    }

    // Rule 14: Played but no other signals → default to IN_PROGRESS
    if playtime > 0.0 {
        return (
            Category::InProgress,
            format!("Played ({playtime}h) but no clear completion signal"),
        );
    }

    (
        Category::InProgress,
        "No classification signals — defaulted to In Progress".into(),
    )
}

/// Classify all games using:
///   1. Manual overrides → always win
///   2. NOT_A_GAME name patterns → override saved
///   3. Saved classifications → reuse as-is
///   4. Rule-based classification → handles all games
pub fn classify_all_games(
    games: &[OwnedGame],
    saved: &HashMap<u64, Classification>,
    overrides: &HashMap<String, String>,
    store_cache: &HashMap<String, StoreDetails>,
) -> Vec<Classification> {
    let mut results: Vec<Classification> = Vec::with_capacity(games.len());

    for game in games {
        let appid_str = game.appid.to_string();

        // Layer 1: Manual overrides always win
        if let Some(override_cat) = overrides.get(&appid_str) {
            let category = match override_cat.as_str() {
                "COMPLETED" => Category::Completed,
                "IN_PROGRESS" => Category::InProgress,
                "ENDLESS" => Category::Endless,
                "NOT_A_GAME" => Category::NotAGame,
                _ => Category::InProgress,
            };
            results.push(Classification {
                appid: game.appid,
                name: game.name.clone(),
                category,
                confidence: "HIGH".into(),
                reason: "Manual override".into(),
            });
            continue;
        }

        // Layer 1.5: NOT_A_GAME name patterns override saved classifications
        if NOT_A_GAME_NAME_PATTERNS.is_match(&game.name) {
            results.push(Classification {
                appid: game.appid,
                name: game.name.clone(),
                category: Category::NotAGame,
                confidence: "HIGH".into(),
                reason: "Rule: name matches not-a-game pattern".into(),
            });
            continue;
        }

        // Layer 2: Reuse saved classifications
        if let Some(saved_class) = saved.get(&game.appid) {
            results.push(saved_class.clone());
            continue;
        }

        // Layer 3: Rule-based classification
        let store_info = store_cache.get(&appid_str);
        let (category, reason) = classify_by_rules(game, store_info);
        let confidence = if reason.contains("Likely") || reason.to_lowercase().contains("default") {
            "MEDIUM"
        } else {
            "HIGH"
        };

        results.push(Classification {
            appid: game.appid,
            name: game.name.clone(),
            category,
            confidence: confidence.into(),
            reason: format!("Rule: {reason}"),
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_a_game_by_type() {
        let game = OwnedGame {
            appid: 1,
            name: "Some DLC".into(),
            playtime_hours: 0.0,
            achievements: None,
        };
        let store = StoreDetails {
            app_type: "dlc".into(),
            genres: vec![],
            categories: vec![],
        };
        let (cat, _) = classify_by_rules(&game, Some(&store));
        assert_eq!(cat, Category::NotAGame);
    }

    #[test]
    fn test_not_a_game_by_name() {
        let game = OwnedGame {
            appid: 2,
            name: "Half-Life Dedicated Server".into(),
            playtime_hours: 0.0,
            achievements: None,
        };
        let (cat, _) = classify_by_rules(&game, None);
        assert_eq!(cat, Category::NotAGame);
    }

    #[test]
    fn test_multiplayer_only() {
        let game = OwnedGame {
            appid: 3,
            name: "Some Shooter".into(),
            playtime_hours: 10.0,
            achievements: None,
        };
        let store = StoreDetails {
            app_type: "game".into(),
            genres: vec!["Action".into()],
            categories: vec!["Multi-player".into()],
        };
        let (cat, _) = classify_by_rules(&game, Some(&store));
        assert_eq!(cat, Category::Endless);
    }

    #[test]
    fn test_unplayed_sp_is_backlog() {
        let game = OwnedGame {
            appid: 4,
            name: "Portal 3".into(),
            playtime_hours: 0.0,
            achievements: None,
        };
        let store = StoreDetails {
            app_type: "game".into(),
            genres: vec!["Puzzle".into()],
            categories: vec!["Single-player".into()],
        };
        let (cat, _) = classify_by_rules(&game, Some(&store));
        assert_eq!(cat, Category::InProgress);
    }
}
