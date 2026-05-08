"""模型别名和多 provider 路由工具。"""

from __future__ import annotations

import copy
import re
from typing import Optional


MODEL_SLOTS = (
    {
        "key": "default",
        "legacy": ("default",),
        "claude_ids": (),
    },
    {
        "key": "opus_4_7",
        "legacy": ("opus",),
        "claude_ids": ("claude-opus-4-7",),
    },
    {
        "key": "opus_4_6",
        "legacy": (),
        "claude_ids": ("claude-opus-4-6",),
    },
    {
        "key": "opus_3",
        "legacy": (),
        "claude_ids": ("claude-3-opus",),
    },
    {
        "key": "sonnet_4_6",
        "legacy": ("sonnet",),
        "claude_ids": ("claude-sonnet-4-6",),
    },
    {
        "key": "sonnet_4_5",
        "legacy": (),
        "claude_ids": ("claude-sonnet-4-5",),
    },
    {
        "key": "haiku_4_5",
        "legacy": ("haiku",),
        "claude_ids": ("claude-haiku-4-5",),
    },
)
MODEL_ORDER = tuple(item["key"] for item in MODEL_SLOTS)
DEFAULT_MODEL_KEY = "default"
LEGACY_MODEL_KEYS = ("default", "sonnet", "opus", "haiku")
MODEL_MAPPING_KEYS = set(MODEL_ORDER) | set(LEGACY_MODEL_KEYS)
CUSTOM_ROUTE_RE = re.compile(r"^claude-[A-Za-z0-9][A-Za-z0-9._-]*$")
CLAUDE_ID_TO_SLOT = {
    claude_id.lower(): slot["key"]
    for slot in MODEL_SLOTS
    for claude_id in slot["claude_ids"]
}
DESKTOP_ROUTE_IDS = tuple(
    claude_id
    for slot in MODEL_SLOTS
    for claude_id in slot["claude_ids"]
)


def empty_model_mappings() -> dict:
    return {key: "" for key in MODEL_ORDER}


def is_safe_custom_route_id(route_id: str) -> bool:
    """Only claude-* route names are safe to expose to Claude Desktop."""
    route_id = str(route_id or "").strip()
    return bool(CUSTOM_ROUTE_RE.fullmatch(route_id))


def normalize_model_mappings(models: Optional[dict]) -> dict:
    """把旧四槽位和新多槽位统一成当前结构，保留安全的自定义 Claude route。"""
    normalized = empty_model_mappings()
    if not isinstance(models, dict):
        return normalized

    source = copy.deepcopy(models)
    default = str(source.get("default") or "").strip()
    normalized["default"] = default

    for slot in MODEL_SLOTS:
        key = slot["key"]
        if key == DEFAULT_MODEL_KEY:
            continue
        for candidate in (key, *slot["legacy"]):
            value = str(source.get(candidate) or "").strip()
            if value:
                normalized[key] = value
                break

    for key, value in source.items():
        route_id = str(key or "").strip()
        source_model = str(value or "").strip()
        if route_id not in MODEL_MAPPING_KEYS and is_safe_custom_route_id(route_id) and source_model:
            normalized[route_id] = source_model
    return normalized


def custom_model_mappings(models: Optional[dict]) -> dict[str, str]:
    """返回安全的自定义 Claude route -> 上游模型映射。"""
    normalized = normalize_model_mappings(models)
    return {
        key: value
        for key, value in normalized.items()
        if key not in MODEL_MAPPING_KEYS and is_safe_custom_route_id(key) and value
    }


def model_mappings_with_legacy_aliases(models: Optional[dict]) -> dict:
    """在新槽位结构上补回旧四槽位别名，供兼容读取。"""
    normalized = normalize_model_mappings(models)
    compat = dict(normalized)
    compat["default"] = normalized.get("default", "")
    compat["sonnet"] = (
        normalized.get("sonnet_4_6")
        or normalized.get("sonnet_4_5")
        or ""
    )
    compat["opus"] = (
        normalized.get("opus_4_7")
        or normalized.get("opus_4_6")
        or normalized.get("opus_3")
        or ""
    )
    compat["haiku"] = (
        normalized.get("haiku_4_5")
        or ""
    )
    return compat


def provider_model_ids(provider: Optional[dict]) -> list[str]:
    """按稳定顺序返回 provider 暴露给 Claude 的真实模型 ID。"""
    if not provider:
        return []
    models = normalize_model_mappings(provider.get("models") or {})
    ordered: list[str] = []
    for key in MODEL_ORDER:
        model_id = str(models.get(key) or "").strip()
        if model_id and model_id not in ordered:
            ordered.append(model_id)
    for model_id in custom_model_mappings(provider.get("models") or {}).values():
        model_id = str(model_id or "").strip()
        if model_id and model_id not in ordered:
            ordered.append(model_id)
    return ordered


def provider_slug(provider: dict) -> str:
    """生成用于 Claude 模型菜单的稳定 provider 前缀。"""
    source = str(provider.get("id") or provider.get("name") or "provider").lower()
    slug = re.sub(r"[^a-z0-9_-]+", "-", source).strip("-_")
    return slug[:56] or "provider"


def model_alias(provider: dict, model_id: str) -> str:
    """把 provider 和真实模型 ID 组合成菜单别名。"""
    return f"{provider_slug(provider)}/{model_id}"


def model_supports_1m(provider: dict, model_id: str) -> bool:
    """判断模型是否应声明 supports1m。"""
    capabilities = provider.get("modelCapabilities") or {}
    if not isinstance(capabilities, dict):
        capabilities = {}
    model_capability = capabilities.get(model_id)
    return "[1m]" in model_id.lower() or (
        isinstance(model_capability, dict)
        and model_capability.get("supports1m") is True
    )


def desktop_model_entries(provider: Optional[dict], use_alias: bool = False) -> list[dict]:
    """生成 Claude Desktop / gateway 可见的显式安全路由模型条目。"""
    if not provider:
        return []
    raw_models = provider.get("models") or {}
    if not isinstance(raw_models, dict):
        raw_models = {}
    normalized = normalize_model_mappings(raw_models)
    provider_name = str(provider.get("name") or provider.get("id") or "Provider")
    entries: list[dict] = []
    seen: set[str] = set()

    def add_entry(route_id: str, source_model: str) -> None:
        route_id = str(route_id or "").strip()
        source_model = str(source_model or "").strip()
        if not route_id or not source_model:
            return
        name = model_alias(provider, route_id) if use_alias else route_id
        if name in seen:
            return
        seen.add(name)
        item = {
            "name": name,
            "displayName": f"{provider_name} / {route_id}" if use_alias else route_id,
            "sourceModel": source_model,
            "providerId": provider.get("id"),
        }
        if model_supports_1m(provider, source_model):
            item["supports1m"] = True
        entries.append(item)

    for slot in MODEL_SLOTS:
        if slot["key"] == DEFAULT_MODEL_KEY or not slot["claude_ids"]:
            continue
        source_model = normalized.get(slot["key"])
        if source_model:
            add_entry(slot["claude_ids"][0], source_model)

    for route_id, source_model in custom_model_mappings(raw_models).items():
        add_entry(route_id, source_model)

    return entries


def provider_model_entries(provider: Optional[dict], use_alias: bool = False) -> list[dict]:
    """生成真实上游模型条目；不要直接用于 Claude Desktop 暴露面。"""
    if not provider:
        return []
    entries: list[dict] = []
    provider_name = str(provider.get("name") or provider.get("id") or "Provider")
    for model_id in provider_model_ids(provider):
        name = model_alias(provider, model_id) if use_alias else model_id
        item = {
            "name": name,
            "displayName": f"{provider_name} / {model_id}" if use_alias else model_id,
            "sourceModel": model_id,
            "providerId": provider.get("id"),
        }
        if model_supports_1m(provider, model_id):
            item["supports1m"] = True
        entries.append(item)
    return entries


def all_provider_model_entries(providers: list[dict]) -> list[dict]:
    """生成所有 provider 的去重安全路由模型条目。"""
    entries: list[dict] = []
    seen: set[str] = set()
    for provider in providers:
        if not provider:
            continue
        for item in desktop_model_entries(provider, use_alias=True):
            name = item["name"]
            if name in seen:
                continue
            seen.add(name)
            entries.append(item)
    return entries


def resolve_model_alias(providers: list[dict], requested_model: str) -> tuple[Optional[dict], str, bool]:
    """把 provider/model 别名解析为具体 provider 和真实模型 ID。"""
    requested = str(requested_model or "")
    if "/" not in requested:
        return None, requested, False
    slug, model_id = requested.split("/", 1)
    if not slug or not model_id:
        return None, requested, False
    for provider in providers:
        if provider_slug(provider) == slug:
            for item in desktop_model_entries(provider):
                if model_id == item.get("name"):
                    return provider, model_id, True
            if model_id in provider_model_ids(provider):
                return provider, model_id, True
            # 允许用户手动写入未出现在映射里的模型 ID。
            return provider, model_id, True
    return None, requested, False


def resolve_requested_model_slot(requested_model: str) -> Optional[str]:
    """把 Claude 请求模型名解析为当前映射槽位。"""
    requested = str(requested_model or "").strip().lower()
    if not requested:
        return None
    mapped = CLAUDE_ID_TO_SLOT.get(requested)
    if mapped:
        return mapped

    # 兼容旧版本 Claude 模型 ID 和未精确列出的家族命名。
    if "haiku" in requested:
        return "haiku"
    if "sonnet" in requested:
        if "4-6" in requested:
            return "sonnet_4_6"
        if "4-5" in requested:
            return "sonnet_4_5"
        return "sonnet"
    if "opus" in requested:
        if "4-7" in requested:
            return "opus_4_7"
        if "4-6" in requested:
            return "opus_4_6"
        if requested.startswith("claude-3") or "-3-" in requested or requested.endswith("-3"):
            return "opus_3"
        return "opus"
    return None


def desktop_route_ids() -> tuple[str, ...]:
    """返回 Claude Desktop 可见的安全 route ID 集合。"""
    return DESKTOP_ROUTE_IDS
