use std::{error::Error, fs, path::Path};

use serde::Serialize;
use serde_json::json;

type DynResult<T> = Result<T, Box<dyn Error>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardModel {
    key: &'static str,
    title: &'static str,
    icon: &'static str,
    source: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderFormModelSlot {
    key: &'static str,
    label: &'static str,
    icon: &'static str,
    icon_class: &'static str,
    source: &'static str,
    #[serde(skip_serializing_if = "is_false")]
    required: bool,
}

#[derive(Clone, Copy)]
struct LegacyCandidates {
    target: &'static str,
    candidates: &'static [&'static str],
}

const DASHBOARD_MODELS: &[DashboardModel] = &[
    DashboardModel {
        key: "sonnet",
        title: "Sonnet",
        icon: "bi-stars",
        source: "claude-sonnet-4-6",
    },
    DashboardModel {
        key: "haiku",
        title: "Haiku",
        icon: "bi-leaf",
        source: "claude-haiku-3-5",
    },
    DashboardModel {
        key: "opus",
        title: "Opus",
        icon: "bi-box",
        source: "claude-opus-4-7",
    },
];

const PROVIDER_FORM_MODEL_SLOTS: &[ProviderFormModelSlot] = &[
    ProviderFormModelSlot {
        key: "default",
        label: "Default",
        icon: "bi-circle-fill",
        icon_class: "default",
        source: "未配置映射时默认使用这一项",
        required: true,
    },
    ProviderFormModelSlot {
        key: "opus_4_7",
        label: "Opus 4.7",
        icon: "bi-box",
        icon_class: "opus",
        source: "claude-opus-4-7",
        required: false,
    },
    ProviderFormModelSlot {
        key: "opus_4_6",
        label: "Opus 4.6",
        icon: "bi-box",
        icon_class: "opus",
        source: "claude-opus-4-6",
        required: false,
    },
    ProviderFormModelSlot {
        key: "opus_3",
        label: "Opus 3",
        icon: "bi-box",
        icon_class: "opus",
        source: "claude-3-opus",
        required: false,
    },
    ProviderFormModelSlot {
        key: "sonnet_4_6",
        label: "Sonnet 4.6",
        icon: "bi-stars",
        icon_class: "sonnet",
        source: "claude-sonnet-4-6",
        required: false,
    },
    ProviderFormModelSlot {
        key: "sonnet_4_5",
        label: "Sonnet 4.5",
        icon: "bi-stars",
        icon_class: "sonnet",
        source: "claude-sonnet-4-5",
        required: false,
    },
    ProviderFormModelSlot {
        key: "haiku_4_5",
        label: "Haiku 4.5",
        icon: "bi-leaf",
        icon_class: "haiku",
        source: "claude-haiku-4-5",
        required: false,
    },
];

const PROVIDER_FORM_DEFAULT_ROWS: &[&str] = &["default", "opus_4_7", "sonnet_4_6", "haiku_4_5"];

const LEGACY_CANDIDATES: &[LegacyCandidates] = &[
    LegacyCandidates {
        target: "opus_4_7",
        candidates: &["opus_4_7", "opus"],
    },
    LegacyCandidates {
        target: "opus_4_6",
        candidates: &["opus_4_6"],
    },
    LegacyCandidates {
        target: "opus_3",
        candidates: &["opus_3"],
    },
    LegacyCandidates {
        target: "sonnet_4_6",
        candidates: &["sonnet_4_6", "sonnet"],
    },
    LegacyCandidates {
        target: "sonnet_4_5",
        candidates: &["sonnet_4_5"],
    },
    LegacyCandidates {
        target: "haiku_4_5",
        candidates: &["haiku_4_5", "haiku"],
    },
];

const CLAUDE_ID_TO_SLOT: &[(&str, &str)] = &[
    ("claude-opus-4-7", "opus_4_7"),
    ("claude-opus-4-6", "opus_4_6"),
    ("claude-3-opus", "opus_3"),
    ("claude-sonnet-4-6", "sonnet_4_6"),
    ("claude-sonnet-4-5", "sonnet_4_5"),
    ("claude-haiku-4-5", "haiku_4_5"),
];

const CCAPI_METHODS: &[&str] = &[
    "getStatus",
    "getProviders",
    "getProviderSecret",
    "getPresets",
    "addProvider",
    "updateProvider",
    "deleteProvider",
    "detectApiFormat",
    "setDefaultProvider",
    "reorderProviders",
    "testProvider",
    "queryProviderUsage",
    "getProviderCompatibility",
    "testProviderPayload",
    "saveModelMappings",
    "fetchProviderModels",
    "fetchProviderModelsPayload",
    "autofillProviderModels",
    "checkModelAvailability",
    "getDesktopStatus",
    "configureDesktop",
    "clearDesktop",
    "restartClaudeDesktop",
    "startProxy",
    "stopProxy",
    "getProxyLogs",
    "getProxyStatus",
    "clearLogs",
    "getSettings",
    "saveSettings",
    "detectLocalProxy",
    "checkUpdate",
    "installUpdate",
    "getUpdateProgress",
    "createBackup",
    "listBackups",
    "exportConfig",
    "importConfig",
    "submitFeedback",
    "getCcSwitchStatus",
    "getCcSwitchProviders",
    "importCcSwitchProviders",
    "getActivities",
    "getModels",
];

pub fn ccapi_methods() -> &'static [&'static str] {
    CCAPI_METHODS
}

pub fn generate_contract_files(root: &Path, check: bool) -> DynResult<()> {
    let frontend_contracts = frontend_contracts_source()?;
    let frontend_target = root.join("frontend/js/generated/contracts.js");
    write_or_check(
        &frontend_target,
        &frontend_contracts,
        check,
        "frontend/js/generated/contracts.js",
    )?;

    let rust_contracts = rust_contracts_source();
    let rust_target = root.join("src-tauri/src/generated/model_contracts.rs");
    write_or_check(
        &rust_target,
        &rust_contracts,
        check,
        "src-tauri/src/generated/model_contracts.rs",
    )?;
    Ok(())
}

fn frontend_contracts_source() -> DynResult<String> {
    let model_order = PROVIDER_FORM_MODEL_SLOTS
        .iter()
        .map(|slot| slot.key)
        .collect::<Vec<_>>();
    let legacy_candidates = LEGACY_CANDIDATES
        .iter()
        .map(|item| json!({ "target": item.target, "candidates": item.candidates }))
        .collect::<Vec<_>>();
    let claude_id_to_slot = CLAUDE_ID_TO_SLOT
        .iter()
        .map(|(claude_id, slot)| json!({ "claudeId": claude_id, "slot": slot }))
        .collect::<Vec<_>>();
    let contracts = json!({
        "modelMeta": DASHBOARD_MODELS,
        "providerFormModelSlots": PROVIDER_FORM_MODEL_SLOTS,
        "providerFormDefaultRows": PROVIDER_FORM_DEFAULT_ROWS,
        "modelOrder": model_order,
        "legacyCandidates": legacy_candidates,
        "claudeIdToSlot": claude_id_to_slot,
        "ccApiMethods": CCAPI_METHODS,
    });
    Ok(format!(
        "// Generated by xtask frontend contracts. Edit xtask/src/contracts.rs instead.\nwindow.CCDS_CONTRACTS = {};\n",
        serde_json::to_string_pretty(&contracts)?
    ))
}

fn rust_contracts_source() -> String {
    format!(
        "// Generated by xtask frontend contracts. Edit xtask/src/contracts.rs instead.\n\n{}\n\n{}\n\n{}\n",
        rust_model_order(),
        rust_legacy_candidates(),
        rust_claude_id_to_slot(),
    )
}

fn rust_model_order() -> String {
    let values = PROVIDER_FORM_MODEL_SLOTS
        .iter()
        .map(|slot| format!("    {:?},", slot.key))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "pub const MODEL_ORDER: [&str; {}] = [\n{}\n];",
        PROVIDER_FORM_MODEL_SLOTS.len(),
        values,
    )
}

fn rust_legacy_candidates() -> String {
    let values = LEGACY_CANDIDATES
        .iter()
        .map(|item| {
            let candidates = item
                .candidates
                .iter()
                .map(|candidate| format!("{candidate:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("    ({:?}, &[{}]),", item.target, candidates)
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "pub const LEGACY_CANDIDATES: [(&str, &[&str]); {}] = [\n{}\n];",
        LEGACY_CANDIDATES.len(),
        values,
    )
}

fn rust_claude_id_to_slot() -> String {
    let values = CLAUDE_ID_TO_SLOT
        .iter()
        .map(|(claude_id, slot)| format!("    ({claude_id:?}, {slot:?}),"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "pub const CLAUDE_ID_TO_SLOT: [(&str, &str); {}] = [\n{}\n];",
        CLAUDE_ID_TO_SLOT.len(),
        values,
    )
}

fn write_or_check(
    target: &Path,
    generated: &str,
    check: bool,
    display_name: &str,
) -> DynResult<()> {
    if check {
        let current = fs::read_to_string(target)?;
        if current != generated {
            return Err(format!(
                "{display_name} is out of date. Run pnpm build to regenerate contract files."
            )
            .into());
        }
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target, generated)?;
    Ok(())
}

fn is_false(value: &bool) -> bool {
    !*value
}
