use std::collections::BTreeMap;

pub use crate::generated::model_contracts::MODEL_ORDER;
use crate::generated::model_contracts::{CLAUDE_ID_TO_SLOT, LEGACY_CANDIDATES};

pub fn empty_model_mappings() -> BTreeMap<String, String> {
    MODEL_ORDER
        .iter()
        .map(|key| ((*key).to_string(), String::new()))
        .collect()
}

pub fn normalize_model_mappings(
    models: Option<&BTreeMap<String, String>>,
) -> BTreeMap<String, String> {
    let mut normalized = empty_model_mappings();
    let Some(source) = models else {
        return normalized;
    };

    normalized.insert(
        "default".to_string(),
        source
            .get("default")
            .map(|value| value.trim().to_string())
            .unwrap_or_default(),
    );

    for (target, candidates) in LEGACY_CANDIDATES {
        if let Some(value) = candidates
            .iter()
            .filter_map(|candidate| source.get(*candidate))
            .map(|value| value.trim())
            .find(|value| !value.is_empty())
        {
            normalized.insert(target.to_string(), value.to_string());
        }
    }

    normalized
}

pub fn model_mappings_with_legacy_aliases(
    models: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let normalized = normalize_model_mappings(Some(models));
    let mut compat = normalized.clone();
    compat.insert(
        "sonnet".to_string(),
        first_model(&normalized, &["sonnet_4_6", "sonnet_4_5", "default"]),
    );
    compat.insert(
        "opus".to_string(),
        first_model(&normalized, &["opus_4_7", "opus_4_6", "opus_3", "default"]),
    );
    compat.insert(
        "haiku".to_string(),
        first_model(&normalized, &["haiku_4_5", "default"]),
    );
    compat
}

fn first_model(models: &BTreeMap<String, String>, keys: &[&str]) -> String {
    keys.iter()
        .filter_map(|key| models.get(*key))
        .find(|value| !value.is_empty())
        .cloned()
        .unwrap_or_default()
}

#[allow(dead_code)]
pub fn resolve_requested_model_slot(requested_model: &str) -> Option<String> {
    let requested = requested_model.trim().to_lowercase();
    if requested.is_empty() {
        return None;
    }
    if let Some((_, slot)) = CLAUDE_ID_TO_SLOT
        .iter()
        .find(|(claude_id, _)| *claude_id == requested)
    {
        return Some((*slot).to_string());
    }
    if requested.contains("haiku") {
        return Some("haiku".to_string());
    }
    if requested.contains("sonnet") {
        if requested.contains("4-6") {
            return Some("sonnet_4_6".to_string());
        }
        if requested.contains("4-5") {
            return Some("sonnet_4_5".to_string());
        }
        return Some("sonnet".to_string());
    }
    if requested.contains("opus") {
        if requested.contains("4-7") {
            return Some("opus_4_7".to_string());
        }
        if requested.contains("4-6") {
            return Some("opus_4_6".to_string());
        }
        if requested.starts_with("claude-3")
            || requested.contains("-3-")
            || requested.ends_with("-3")
        {
            return Some("opus_3".to_string());
        }
        return Some("opus".to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_legacy_slots() {
        let mut source = BTreeMap::new();
        source.insert("sonnet".to_string(), "deepseek-v4-pro".to_string());
        source.insert("haiku".to_string(), "deepseek-v4-flash".to_string());

        let normalized = normalize_model_mappings(Some(&source));

        assert_eq!(normalized["sonnet_4_6"], "deepseek-v4-pro");
        assert_eq!(normalized["haiku_4_5"], "deepseek-v4-flash");
    }

    #[test]
    fn adds_legacy_aliases_from_new_slots() {
        let mut source = empty_model_mappings();
        source.insert("default".to_string(), "fallback".to_string());
        source.insert("opus_4_7".to_string(), "opus-model".to_string());

        let compat = model_mappings_with_legacy_aliases(&source);

        assert_eq!(compat["opus"], "opus-model");
        assert_eq!(compat["haiku"], "fallback");
    }

    #[test]
    fn resolves_current_and_legacy_claude_model_ids() {
        assert_eq!(
            resolve_requested_model_slot("claude-sonnet-4-6").as_deref(),
            Some("sonnet_4_6")
        );
        assert_eq!(
            resolve_requested_model_slot("claude-haiku-3-5").as_deref(),
            Some("haiku")
        );
        assert_eq!(
            resolve_requested_model_slot("claude-opus-4-7").as_deref(),
            Some("opus_4_7")
        );
    }
}
