use chrono::Utc;
use regex::Regex;
use uuid::Uuid;

use crate::types::ExtractedFact;

/// Regex-based fact and entity extractor. Runs deterministically with
/// zero LLM calls. Extracts:
/// - Preferences ("I prefer X", "I like X", "my favorite is X")
/// - Decisions ("I decided to X", "we chose X", "let's go with X")
/// - Named entities (capitalized multi-word sequences)
/// - Key-value patterns ("X is Y", "X = Y")
pub struct RegexExtractor;

impl RegexExtractor {
    /// Extract structured facts from text content.
    pub fn extract(content: &str, source_memory_id: Uuid) -> Vec<ExtractedFact> {
        let mut facts = Vec::new();
        let now = Utc::now();

        // Preference patterns.
        let preference_patterns = [
            r"(?i)\b(?:i|we)\s+(?:prefer|like|love|enjoy|want)\s+(.+?)(?:\.|$|\n)",
            r"(?i)\bmy\s+(?:favorite|preferred)\s+(?:\w+\s+)?is\s+(.+?)(?:\.|$|\n)",
            r"(?i)\b(?:i|we)\s+(?:always|usually)\s+(?:use|choose)\s+(.+?)(?:\.|$|\n)",
        ];

        for pattern in &preference_patterns {
            if let Ok(re) = Regex::new(pattern) {
                for cap in re.captures_iter(content) {
                    if let Some(obj) = cap.get(1) {
                        let object = obj.as_str().trim().to_string();
                        if object.len() >= 2 && object.len() <= 200 {
                            facts.push(ExtractedFact {
                                id: Uuid::new_v4(),
                                source_memory_id,
                                fact_type: "preference".to_string(),
                                subject: "user".to_string(),
                                predicate: "prefers".to_string(),
                                object,
                                confidence: 0.7,
                                valid_from: now,
                                valid_to: None,
                                extraction_method: "regex".to_string(),
                                created_at: now,
                            });
                        }
                    }
                }
            }
        }

        // Decision patterns.
        let decision_patterns = [
            r"(?i)\b(?:i|we)\s+(?:decided|chose|picked|selected|went with)\s+(.+?)(?:\.|$|\n)",
            r"(?i)\blet'?s?\s+(?:go with|use|choose)\s+(.+?)(?:\.|$|\n)",
        ];

        for pattern in &decision_patterns {
            if let Ok(re) = Regex::new(pattern) {
                for cap in re.captures_iter(content) {
                    if let Some(obj) = cap.get(1) {
                        let object = obj.as_str().trim().to_string();
                        if object.len() >= 2 && object.len() <= 200 {
                            facts.push(ExtractedFact {
                                id: Uuid::new_v4(),
                                source_memory_id,
                                fact_type: "decision".to_string(),
                                subject: "user".to_string(),
                                predicate: "decided".to_string(),
                                object,
                                confidence: 0.8,
                                valid_from: now,
                                valid_to: None,
                                extraction_method: "regex".to_string(),
                                created_at: now,
                            });
                        }
                    }
                }
            }
        }

        // Key-value / "X is Y" patterns (limited to short, concrete statements).
        if let Ok(re) =
            Regex::new(r"(?i)\b([A-Z][a-z]+(?:\s+[A-Z][a-z]+)*)\s+is\s+(?:a\s+)?(\w[\w\s]{1,50}?)(?:\.|$|\n)")
        {
            for cap in re.captures_iter(content) {
                if let (Some(subj), Some(obj)) = (cap.get(1), cap.get(2)) {
                    let subject = subj.as_str().trim().to_string();
                    let object = obj.as_str().trim().to_string();
                    if subject.len() >= 2 && object.len() >= 2 {
                        facts.push(ExtractedFact {
                            id: Uuid::new_v4(),
                            source_memory_id,
                            fact_type: "entity".to_string(),
                            subject,
                            predicate: "is".to_string(),
                            object,
                            confidence: 0.6,
                            valid_from: now,
                            valid_to: None,
                            extraction_method: "regex".to_string(),
                            created_at: now,
                        });
                    }
                }
            }
        }

        facts
    }

    /// Extract potential named entities from text.
    /// Returns unique entity strings (e.g., "John Smith", "Project Alpha").
    pub fn extract_entities(content: &str) -> Vec<String> {
        let mut entities = Vec::new();

        // Multi-word capitalized sequences (potential proper nouns).
        if let Ok(re) = Regex::new(r"\b([A-Z][a-z]+(?:\s+[A-Z][a-z]+)+)\b") {
            for cap in re.captures_iter(content) {
                if let Some(m) = cap.get(1) {
                    let entity = m.as_str().to_string();
                    if !entities.contains(&entity) {
                        entities.push(entity);
                    }
                }
            }
        }

        // Single capitalized words that aren't at sentence start (heuristic).
        // We check by looking for words preceded by lowercase or punctuation.
        if let Ok(re) = Regex::new(r"(?:^|[.!?]\s+)\w") {
            let sentence_starts: Vec<usize> = re
                .find_iter(content)
                .map(|m| m.end().saturating_sub(1))
                .collect();

            if let Ok(cap_re) = Regex::new(r"\b([A-Z][a-z]{2,})\b") {
                for m in cap_re.find_iter(content) {
                    // Skip if this is at a sentence start.
                    if sentence_starts.contains(&m.start()) {
                        continue;
                    }
                    let word = m.as_str().to_string();
                    if !entities.contains(&word) {
                        entities.push(word);
                    }
                }
            }
        }

        entities
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_preferences() {
        let id = Uuid::new_v4();
        let content = "I prefer Rust over Python. My favorite editor is Neovim.";
        let facts = RegexExtractor::extract(content, id);

        let prefs: Vec<_> = facts
            .iter()
            .filter(|f| f.fact_type == "preference")
            .collect();
        assert!(!prefs.is_empty());
        assert!(prefs.iter().any(|f| f.object.contains("Rust")));
    }

    #[test]
    fn test_extract_decisions() {
        let id = Uuid::new_v4();
        let content = "We decided to use PostgreSQL for the database.";
        let facts = RegexExtractor::extract(content, id);

        let decisions: Vec<_> = facts
            .iter()
            .filter(|f| f.fact_type == "decision")
            .collect();
        assert!(!decisions.is_empty());
        assert!(decisions.iter().any(|f| f.object.contains("PostgreSQL")));
    }

    #[test]
    fn test_extract_entities() {
        let content = "I talked to John Smith about Project Alpha yesterday.";
        let entities = RegexExtractor::extract_entities(content);
        assert!(entities.contains(&"John Smith".to_string()));
        assert!(entities.contains(&"Project Alpha".to_string()));
    }

    #[test]
    fn test_empty_content() {
        let id = Uuid::new_v4();
        let facts = RegexExtractor::extract("", id);
        assert!(facts.is_empty());
        let entities = RegexExtractor::extract_entities("");
        assert!(entities.is_empty());
    }
}
