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
        write_candidate(&mut card, candidate);
        write_incoming(&mut card, incoming);
        card.push_str("- Detected Numerical Deltas:\n");

        let mut found = DetectedDeltas::default();
        found.detect_attribute_deltas(&mut card, candidate, incoming, config);
        found.apply_ratio_rules(&mut card, candidate, incoming, config);
        if !found.any {
            card.push_str("  * None detected\n");
        }

        let h3_ring_distance = write_spatial_proximity(&mut card, candidate, incoming);
        card.push_str(CLASSIFICATION_TASK);

        SemanticDeltaSummary {
            attribute_deltas: found.deltas,
            weight_match_pct: found.weight_pct,
            scale_multiplier: found.multiplier,
            h3_ring_distance,
            comparison_card: card,
        }
    }
}

const CLASSIFICATION_TASK: &str = "\nClassification Task:\n\
Determine if this is:\n\
1. An attribute update to anchor\n\
2. A new child/variant attached under anchor\n\
3. A distinct category branch\n";

/// The numeric deltas found so far, with the primary weight match and scale.
#[derive(Default)]
struct DetectedDeltas {
    deltas: HashMap<String, AttributeDelta>,
    weight_pct: Option<f64>,
    multiplier: Option<f64>,
    any: bool,
}

/// A candidate attribute and the incoming value it is compared with.
struct NumericPair<'a> {
    key: &'a str,
    candidate_val: &'a serde_json::Value,
    incoming_val: &'a serde_json::Value,
    candidate_num: f64,
    incoming_num: f64,
}

impl DetectedDeltas {
    /// Direct numeric matches on the same key or a configured prefix.
    fn detect_attribute_deltas(
        &mut self,
        card: &mut String,
        candidate: &CandidateEntity,
        incoming: &IncomingEntity,
        config: &DeltaSynthesizerConfig,
    ) {
        for (key, candidate_val) in &candidate.attributes {
            let incoming_val = find_incoming_value(incoming, &config.attribute_match_prefixes, key);
            let pair = incoming_val.and_then(|incoming_val| {
                Some(NumericPair {
                    key,
                    candidate_val,
                    incoming_val,
                    candidate_num: candidate_val.as_f64()?,
                    incoming_num: incoming_val.as_f64()?,
                })
            });
            if let Some(pair) = pair.filter(|p| p.candidate_num > 0.0) {
                self.record(card, &pair);
            }
        }
    }

    fn record(&mut self, card: &mut String, pair: &NumericPair) {
        let (cand, inc) = (pair.candidate_num, pair.incoming_num);
        let match_ratio = 1.0 - ((cand - inc).abs() / cand);
        let pct = (match_ratio.max(0.0) * 100.0).round();
        let multiplier = inc / cand;

        card.push_str(&format!(
            "  * {}: cand={:.1}, incoming={:.1} ({:.0}% match, {:.1}x scale)\n",
            pair.key, cand, inc, pct, multiplier
        ));
        self.deltas.insert(
            pair.key.to_string(),
            AttributeDelta {
                key: pair.key.to_string(),
                candidate_val: Some(pair.candidate_val.clone()),
                incoming_val: Some(pair.incoming_val.clone()),
                match_pct: Some(pct),
                scale_multiplier: Some(multiplier),
            },
        );
        if self.weight_pct.is_none() && multiplier == 1.0 {
            self.weight_pct = Some(pct);
        }
        if self.multiplier.is_none() && multiplier > 1.0 {
            self.multiplier = Some(multiplier);
        }
        self.any = true;
    }

    /// User-configured proportional scalar ratio rules.
    fn apply_ratio_rules(
        &mut self,
        card: &mut String,
        candidate: &CandidateEntity,
        incoming: &IncomingEntity,
        config: &DeltaSynthesizerConfig,
    ) {
        for rule in &config.ratio_rules {
            let cand_num = numeric_attr(&candidate.attributes, &rule.candidate_key);
            let inc_num = numeric_attr(&incoming.attributes, &rule.incoming_key);
            if let (Some(cand_num), Some(inc_num)) = (cand_num, inc_num)
                && cand_num > 0.0
            {
                let mult = inc_num / cand_num;
                self.multiplier = Some(mult);
                let pack_note = pack_note(rule, incoming, mult);
                card.push_str(&format!(
                    "  * Multiplier Detected ({}): {} (Total: {:.1}, {:.0}x scale)\n",
                    rule.label, pack_note, inc_num, mult
                ));
                self.any = true;
            }
        }
    }
}

fn numeric_attr(attributes: &HashMap<String, serde_json::Value>, key: &str) -> Option<f64> {
    attributes.get(key).and_then(|v| v.as_f64())
}

/// The incoming value under the same key, else under the first configured prefix that has one.
fn find_incoming_value<'a>(
    incoming: &'a IncomingEntity,
    prefixes: &[String],
    key: &str,
) -> Option<&'a serde_json::Value> {
    incoming.attributes.get(key).or_else(|| {
        prefixes
            .iter()
            .find_map(|prefix| incoming.attributes.get(&format!("{prefix}{key}")))
    })
}

/// "12 units" from the rule's count attribute, else the multiplier.
fn pack_note(rule: &ScalarRatioRule, incoming: &IncomingEntity, mult: f64) -> String {
    rule.count_key
        .as_ref()
        .and_then(|ck| incoming.attributes.get(ck))
        .and_then(|v| v.as_u64())
        .map(|c| format!("{c} units"))
        .unwrap_or_else(|| format!("{mult:.0}x units"))
}

fn write_candidate(card: &mut String, candidate: &CandidateEntity) {
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
    write_sorted_attributes(card, &candidate.attributes);
}

fn write_incoming(card: &mut String, incoming: &IncomingEntity) {
    card.push_str("\nIncoming Entity to Classify:\n");
    card.push_str(&format!("- Text: \"{}\"\n", incoming.text));
    write_sorted_attributes(card, &incoming.attributes);
}

/// Attributes in deterministic sorted order.
fn write_sorted_attributes(card: &mut String, attributes: &HashMap<String, serde_json::Value>) {
    let sorted: BTreeMap<_, _> = attributes.iter().collect();
    for (k, v) in &sorted {
        card.push_str(&format!("- {}: {}\n", k, format_attr_val(v)));
    }
}

/// Appends the geospatial proximity line and returns the H3 ring distance, when both have a cell.
fn write_spatial_proximity(
    card: &mut String,
    candidate: &CandidateEntity,
    incoming: &IncomingEntity,
) -> Option<u32> {
    let (cand_cell, inc_cell) = candidate.h3_cell.zip(incoming.h3_cell)?;
    let ring_dist = GeospatialEncoder::grid_distance(cand_cell, inc_cell).unwrap_or(u32::MAX);
    let loc_desc = if ring_dist == 0 {
        "Co-located in same spatial zone (H3 ring distance: 0)".to_string()
    } else {
        format!("Adjacent regional zone (H3 ring distance: {ring_dist})")
    };
    card.push_str(&format!("- Spatial Proximity: {loc_desc}\n"));
    Some(ring_dist)
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

    #[test]
    fn the_default_configuration_matches_detected_prefixes_and_reports_missing_deltas() {
        let candidate = CandidateEntity::new("Unit").with_attribute("weight", 100.0);
        let doubled = IncomingEntity::new("Pack").with_attribute("detected_weight", 200.0);
        let summary = SemanticDeltaSynthesizer::synthesize(&candidate, &doubled);
        assert_eq!(summary.scale_multiplier, Some(2.0));
        assert!(summary.comparison_card.contains("2.0x scale"));

        let nothing_alike = IncomingEntity::new("Other").with_attribute("colour", "red");
        let summary = SemanticDeltaSynthesizer::synthesize(&candidate, &nothing_alike);
        assert!(summary.comparison_card.contains("None detected"));
        assert!(summary.scale_multiplier.is_none() && summary.h3_ring_distance.is_none());
    }

    #[test]
    fn identical_values_set_the_weight_match_and_a_second_multiplier_is_ignored() {
        let candidate = CandidateEntity::new("Unit")
            .with_attribute("a", 10.0)
            .with_attribute("b", 10.0);
        let incoming = IncomingEntity::new("Pack")
            .with_attribute("a", 10.0)
            .with_attribute("b", 30.0);
        let summary = SemanticDeltaSynthesizer::synthesize(&candidate, &incoming);
        assert_eq!(summary.weight_match_pct, Some(100.0));
        assert_eq!(summary.scale_multiplier, Some(3.0));
    }
}
