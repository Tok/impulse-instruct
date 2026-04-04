// ─── llm/instructions.rs ─────────────────────────────────────────────────────
// Instruction set for the mock LLM backend.
// Loaded once from `instructions.json` at the project root (with embedded
// fallback), then queried via word-overlap scoring.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

static INSTRUCTION_SET: OnceLock<InstructionSet> = OnceLock::new();

// Embedded default — always available even when the file is missing.
const DEFAULT_JSON: &str = include_str!("../../instructions.json");

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Instruction {
    pub id: String,
    /// Phrases/words that trigger this instruction.
    /// Multi-word phrases score higher than single words (score = word count).
    pub keywords: Vec<String>,
    /// Human-readable summary placed in `_comment` of the output JSON.
    pub comment: String,
    /// The parameter update JSON to emit when this instruction is matched.
    pub params: serde_json::Value,
}

pub struct InstructionSet(Vec<Instruction>);

// ─── InstructionSet ───────────────────────────────────────────────────────────

impl InstructionSet {
    /// Singleton accessor — loads once, reused for the lifetime of the process.
    pub fn get() -> &'static InstructionSet {
        INSTRUCTION_SET.get_or_init(|| {
            let json = std::fs::read_to_string("instructions.json")
                .unwrap_or_else(|_| DEFAULT_JSON.to_string());
            let instructions: Vec<Instruction> = serde_json::from_str(&json).unwrap_or_else(|e| {
                log::warn!("instructions.json parse error: {e} — using embedded defaults");
                serde_json::from_str(DEFAULT_JSON).unwrap_or_default()
            });
            InstructionSet(instructions)
        })
    }

    /// Return the best-matching instruction for `prompt`, or `None` if no
    /// keyword scores above zero.
    ///
    /// Scoring: for every keyword phrase that appears as a substring in the
    /// lowercased prompt, add the number of words in that phrase to the score.
    /// Multi-word phrases ("no claps", "add reverb") therefore outweigh
    /// ambiguous single-word matches, which handles negation/removal variants.
    pub fn find_best_match(&self, prompt: &str) -> Option<&Instruction> {
        let lower = prompt.to_lowercase();
        let mut best: Option<(&Instruction, usize)> = None;
        for inst in &self.0 {
            let score: usize = inst
                .keywords
                .iter()
                .filter(|kw| lower.contains(kw.as_str()))
                .map(|kw| kw.split_whitespace().count())
                .sum();
            if score > 0 && best.is_none_or(|(_, s)| score > s) {
                best = Some((inst, score));
            }
        }
        best.map(|(inst, _)| inst)
    }

    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = &Instruction> {
        self.0.iter()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}
