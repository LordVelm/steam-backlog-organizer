//! Runtime text embedding via model2vec-rs (potion-base-8M, 256-dim static
//! embeddings — pure Rust, no ONNX runtime). Must stay in the SAME embedding
//! space as `scripts/build_catalog.py`: same model, same composite text format.
//! The `embed_parity` test pins this cross-language contract.

use crate::catalog::EMBED_DIM;
use model2vec_rs::model::StaticModel;
use std::path::Path;

pub struct Embedder {
    model: StaticModel,
}

impl std::fmt::Debug for Embedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Embedder(potion-base-8M)")
    }
}

/// Compose the text embedded per game — MUST match build_catalog.py:
/// `", ".join(tags[:15]) + ". " + ", ".join(genres) + ". " + short_desc[:300]`
pub fn compose_embed_text(tags: &[String], genres: &[String], short_desc: &str) -> String {
    let tags_part = tags
        .iter()
        .take(15)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let genres_part = genres.join(", ");
    let desc: String = short_desc.chars().take(300).collect();
    format!("{tags_part}. {genres_part}. {desc}")
}

impl Embedder {
    /// Load from a local model directory containing model.safetensors,
    /// tokenizer.json, config.json.
    pub fn load(dir: &Path) -> Result<Embedder, String> {
        let model = StaticModel::from_pretrained(dir, None, Some(true), None)
            .map_err(|e| format!("load embed model from {}: {e}", dir.display()))?;
        Ok(Embedder { model })
    }

    /// Embed one text; always returns an L2-normalized vector (zero stays zero).
    pub fn embed(&self, text: &str) -> [f32; EMBED_DIM] {
        let v = self.model.encode_single(text);
        let mut out = [0.0f32; EMBED_DIM];
        for (o, x) in out.iter_mut().zip(v.iter()) {
            *o = *x;
        }
        let norm: f32 = out.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for o in out.iter_mut() {
                *o /= norm;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn model_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/potion-base-8M")
    }

    #[test]
    fn embed_parity_with_python() {
        // fixtures/embed_parity.json is emitted by the Python model2vec impl;
        // the Rust embedder must reproduce those vectors (same space as catalog).
        let dir = model_dir();
        assert!(
            dir.join("model.safetensors").exists(),
            "resources/potion-base-8M missing — copy from scripts (see scripts/README.md)"
        );
        let embedder = Embedder::load(&dir).expect("embedder loads");

        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/embed_parity.json");
        let entries: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
        assert!(!entries.is_empty());

        for entry in entries {
            let text = entry["text"].as_str().unwrap();
            let expected: Vec<f32> = entry["vector"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect();
            let got = embedder.embed(text);
            // cosine between Rust and Python vectors must be ~1.0
            let dot: f32 = got.iter().zip(expected.iter()).map(|(a, b)| a * b).sum();
            assert!(
                dot > 0.999,
                "embed parity broken for {text:?}: cosine {dot}"
            );
        }
    }

    #[test]
    fn compose_matches_python_format() {
        let text = compose_embed_text(
            &["Roguelite".into(), "Action".into()],
            &["Indie".into()],
            "Death is progress.",
        );
        assert_eq!(text, "Roguelite, Action. Indie. Death is progress.");
    }
}
