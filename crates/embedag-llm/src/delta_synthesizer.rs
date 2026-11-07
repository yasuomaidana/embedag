use embedag_core::encoder::geospatial::GeospatialEncoder;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::HashMap;

/// Adds the `with_attribute` and `with_h3_cell` builders to a struct with `attributes` and `h3_cell`
/// fields; `$who` names the entity in the docs.
macro_rules! attribute_builders {
    ($who:ident) => {
        #[doc = concat!("Adds a dynamic attribute to the ", stringify!($who), " entity.")]
        pub fn with_attribute(
            mut self,
            key: impl Into<String>,
            value: impl Into<serde_json::Value>,
        ) -> Self {
            self.attributes.insert(key.into(), value.into());
            self
        }

        /// Sets an optional H3 geospatial index cell.
        pub fn with_h3_cell(mut self, cell: u64) -> Self {
            self.h3_cell = Some(cell);
            self
        }
    };
}

/// Structured representation of a candidate anchor node from the graph.
/// Structure-agnostic entity holding arbitrary attributes, hierarchy lineage, and optional spatial coordinates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidateEntity {
    pub key: Option<String>,
    pub name: String,
    pub hierarchy: Vec<String>,
    pub attributes: HashMap<String, serde_json::Value>,
    pub h3_cell: Option<u64>,
}

impl CandidateEntity {
    /// Creates a new candidate anchor entity with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            key: None,
            name: name.into(),
            hierarchy: Vec::new(),
            attributes: HashMap::new(),
            h3_cell: None,
        }
    }

    /// Sets the business or graph key of the candidate node.
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Sets the hierarchical taxonomy path of the candidate node.
    pub fn with_hierarchy(mut self, hierarchy: Vec<String>) -> Self {
        self.hierarchy = hierarchy;
        self
    }

    attribute_builders!(candidate);
}

/// Structured representation of an incoming entity to be resolved or classified against the graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IncomingEntity {
    pub text: String,
    pub attributes: HashMap<String, serde_json::Value>,
    pub h3_cell: Option<u64>,
}

impl IncomingEntity {
    /// Creates a new incoming entity with the given raw text representation.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            attributes: HashMap::new(),
            h3_cell: None,
        }
    }

    attribute_builders!(incoming);
}

/// Computed delta comparison metrics for a specific attribute between candidate and incoming entity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AttributeDelta {
    pub key: String,
    pub candidate_val: Option<serde_json::Value>,
    pub incoming_val: Option<serde_json::Value>,
    pub match_pct: Option<f64>,
    pub scale_multiplier: Option<f64>,
}

/// Pre-computed semantic and numerical delta metrics between candidate and incoming entity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticDeltaSummary {
    pub attribute_deltas: HashMap<String, AttributeDelta>,
    pub weight_match_pct: Option<f64>,
    pub scale_multiplier: Option<f64>,
    pub h3_ring_distance: Option<u32>,
    pub comparison_card: String,
}

/// Type aliases for backwards compatibility with earlier versions.
pub type CandidateItem = CandidateEntity;
pub type IncomingItem = IncomingEntity;

/// User-defined proportional or unit-scaling comparison rule between candidate and incoming attributes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScalarRatioRule {
    /// Attribute key expected on the candidate node (e.g., "unit_quantity", "base_price", "dosage").
    pub candidate_key: String,
    /// Attribute key expected on the incoming entity (e.g., "total_quantity", "bundle_price", "bulk_dosage").
    pub incoming_key: String,
    /// Optional attribute key on incoming representing the unit multiplier / pack count.
    pub count_key: Option<String>,
    /// Semantic label reported in the LLM comparison card.
    pub label: String,
}

impl ScalarRatioRule {
    /// Creates a new ratio scaling rule.
    pub fn new(
        candidate_key: impl Into<String>,
        incoming_key: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            candidate_key: candidate_key.into(),
            incoming_key: incoming_key.into(),
            count_key: None,
            label: label.into(),
        }
    }

    /// Attaches an optional pack count or multiplier attribute key.
    pub fn with_count_key(mut self, key: impl Into<String>) -> Self {
        self.count_key = Some(key.into());
        self
    }
}

/// Configuration governing attribute comparison heuristics and proportional scale rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeltaSynthesizerConfig {
    /// Prefix or alias patterns to probe when searching for matching incoming attributes.
    pub attribute_match_prefixes: Vec<String>,
    /// Configurable scaling rules defined by developers for their specific domain.
    pub ratio_rules: Vec<ScalarRatioRule>,
}

impl Default for DeltaSynthesizerConfig {
    fn default() -> Self {
        Self {
            attribute_match_prefixes: vec!["detected_".to_string()],
            ratio_rules: Vec::new(),
        }
    }
}

impl DeltaSynthesizerConfig {
    /// Constructs an empty configuration with no rules or prefixes.
    pub fn empty() -> Self {
        Self {
            attribute_match_prefixes: Vec::new(),
            ratio_rules: Vec::new(),
        }
    }

    /// Registers a custom attribute match prefix (e.g., "detected_", "total_", "parsed_").
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.attribute_match_prefixes.push(prefix.into());
        self
    }

    /// Registers a domain-specific proportional scaling rule.
    pub fn with_ratio_rule(mut self, rule: ScalarRatioRule) -> Self {
        self.ratio_rules.push(rule);
        self
    }
}

/// Synthesizes mathematical comparison cards for LLM prompts to prevent numerical hallucination.
pub struct SemanticDeltaSynthesizer;

impl SemanticDeltaSynthesizer {
    /// Compares a candidate node against an incoming entity using default domain-agnostic settings.
    pub fn synthesize(
        candidate: &CandidateEntity,
        incoming: &IncomingEntity,
    ) -> SemanticDeltaSummary {
        Self::synthesize_with_config(candidate, incoming, &DeltaSynthesizerConfig::default())
    }

    /// Compares a candidate node against an incoming entity using developer-provided configuration.
    pub fn synthesize_with_config(
        candidate: &CandidateEntity,
        incoming: &IncomingEntity,
        config: &DeltaSynthesizerConfig,
    ) -> SemanticDeltaSummary {
        let mut card = String::new();
        let mut attribute_deltas = HashMap::new();
        let mut primary_weight_pct = None;
        let mut primary_multiplier = None;

        // 1. Candidate summary
        card.push_str("Candidate Anchor Node:\n");
        let key_str = candidate
            .key
            .as_deref()
            .map(|s| format!(" (Key: {s})"))
            .unwrap_or_default();
        card.push_str(&format!("- Name: {}{}\n", candidate.name, key_str));

        if !candidate.hierarchy.is_empty() {
            card.push_str(&format!(
                "- Hierarchy: {}\n",
                candidate.hierarchy.join(" -> ")
            ));
        }

        // Output candidate attributes in deterministic sorted order
        let sorted_cand_attrs: BTreeMap<_, _> = candidate.attributes.iter().collect();
        for (k, v) in &sorted_cand_attrs {
            card.push_str(&format!("- {}: {}\n", k, format_attr_val(v)));
        }

        // 2. Incoming entity summary
        card.push_str("\nIncoming Entity to Classify:\n");
        card.push_str(&format!("- Text: \"{}\"\n", incoming.text));

        let sorted_inc_attrs: BTreeMap<_, _> = incoming.attributes.iter().collect();
        for (k, v) in &sorted_inc_attrs {
            card.push_str(&format!("- {}: {}\n", k, format_attr_val(v)));
        }

        card.push_str("- Detected Numerical Deltas:\n");

        // 3. Dynamic numeric attribute delta detection
        let mut detected_any_delta = false;

        // Check for direct numeric matches on same key or configured prefixes
        for (key, cand_val) in &candidate.attributes {
            if let Some(cand_num) = cand_val.as_f64() {
                let mut inc_val = incoming.attributes.get(key);
                if inc_val.is_none() {
                    for prefix in &config.attribute_match_prefixes {
                        if let Some(v) = incoming.attributes.get(&format!("{prefix}{key}")) {
                            inc_val = Some(v);
                            break;
                        }
                    }
                }

                if let Some(inc_num) = inc_val.and_then(|v| v.as_f64())
                    && cand_num > 0.0
                {
                    let match_ratio = 1.0 - ((cand_num - inc_num).abs() / cand_num);
                    let pct = (match_ratio.max(0.0) * 100.0).round();
                    let multiplier = inc_num / cand_num;

                    card.push_str(&format!(
                        "  * {}: cand={:.1}, incoming={:.1} ({:.0}% match, {:.1}x scale)\n",
                        key, cand_num, inc_num, pct, multiplier
                    ));

                    attribute_deltas.insert(
                        key.clone(),
                        AttributeDelta {
                            key: key.clone(),
                            candidate_val: Some(cand_val.clone()),
                            incoming_val: inc_val.cloned(),
                            match_pct: Some(pct),
                            scale_multiplier: Some(multiplier),
                        },
                    );

                    if primary_weight_pct.is_none() && multiplier == 1.0 {
                        primary_weight_pct = Some(pct);
                    }
                    if primary_multiplier.is_none() && multiplier > 1.0 {
                        primary_multiplier = Some(multiplier);
                    }
                    detected_any_delta = true;
                }
            }
        }

        // Evaluate user-configured proportional scalar ratio rules
        for rule in &config.ratio_rules {
            if let (Some(cand_num), Some(inc_num)) = (
                candidate
                    .attributes
                    .get(&rule.candidate_key)
                    .and_then(|v| v.as_f64()),
                incoming
                    .attributes
                    .get(&rule.incoming_key)
                    .and_then(|v| v.as_f64()),
            ) && cand_num > 0.0
            {
                let mult = inc_num / cand_num;
                primary_multiplier = Some(mult);
                let pack_note = rule
                    .count_key
                    .as_ref()
                    .and_then(|ck| incoming.attributes.get(ck))
                    .and_then(|v| v.as_u64())
                    .map(|c| format!("{c} units"))
                    .unwrap_or_else(|| format!("{:.0}x units", mult));

                card.push_str(&format!(
                    "  * Multiplier Detected ({}): {} (Total: {:.1}, {:.0}x scale)\n",
                    rule.label, pack_note, inc_num, mult
                ));
                detected_any_delta = true;
            }
        }

        if !detected_any_delta {
            card.push_str("  * None detected\n");
        }

        // 4. Geospatial proximity
        let mut h3_ring_distance = None;
        if let (Some(cand_cell), Some(inc_cell)) = (candidate.h3_cell, incoming.h3_cell) {
            let ring_dist =
                GeospatialEncoder::grid_distance(cand_cell, inc_cell).unwrap_or(u32::MAX);
            h3_ring_distance = Some(ring_dist);
            let loc_desc = if ring_dist == 0 {
                "Co-located in same spatial zone (H3 ring distance: 0)".to_string()
            } else {
                format!("Adjacent regional zone (H3 ring distance: {ring_dist})")
            };
            card.push_str(&format!("- Spatial Proximity: {}\n", loc_desc));
        }

        card.push_str("\nClassification Task:\n");
        card.push_str("Determine if this is:\n");
        card.push_str("1. An attribute update to anchor\n");
        card.push_str("2. A new child/variant attached under anchor\n");
        card.push_str("3. A distinct category branch\n");

        SemanticDeltaSummary {
            attribute_deltas,
            weight_match_pct: primary_weight_pct,
            scale_multiplier: primary_multiplier,
            h3_ring_distance,
            comparison_card: card,
        }
    }
}

fn format_attr_val(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Array(arr) => format!("[{} items]", arr.len()),
        serde_json::Value::Object(obj) => format!("{{{} fields}}", obj.len()),
        serde_json::Value::Null => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_delta_prompt_synthesis() {
        let candidate = CandidateEntity::new("Sample Unit Product")
            .with_key("SKU-100")
            .with_hierarchy(vec!["Category".into(), "Subcategory".into()])
            .with_attribute("unit_measure", 50.0)
            .with_attribute("packaging", "Single Unit")
            .with_h3_cell(0x8928308280fffff);

        let incoming = IncomingEntity::new("Sample Pack of 12")
            .with_attribute("total_measure", 600.0)
            .with_attribute("detected_unit_measure", 50.0)
            .with_attribute("unit_measure", 50.0)
            .with_attribute("pack_count", 12)
            .with_h3_cell(0x8928308280fffff);

        let config = DeltaSynthesizerConfig::empty()
            .with_prefix("detected_")
            .with_ratio_rule(
                ScalarRatioRule::new("unit_measure", "total_measure", "Proportional Unit Scaling")
                    .with_count_key("pack_count"),
            );

        let summary =
            SemanticDeltaSynthesizer::synthesize_with_config(&candidate, &incoming, &config);

        // Assert 12x multiplier calculation
        assert_eq!(summary.scale_multiplier, Some(12.0));
        assert_eq!(summary.h3_ring_distance, Some(0));

        // Check emitted comparison card contents
        assert!(summary.comparison_card.contains("Candidate Anchor Node:"));
        assert!(
            summary
                .comparison_card
                .contains("Sample Unit Product (Key: SKU-100)")
        );
        assert!(summary.comparison_card.contains("12 units"));
        assert!(summary.comparison_card.contains("12x scale"));
        assert!(summary.comparison_card.contains("H3 ring distance: 0"));
    }
}
