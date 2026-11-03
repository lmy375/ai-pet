"""评测用哪只模型：env 优先，缺的回落到主人真实 config.yaml 的当前 Agent。"""

from __future__ import annotations

import os
import sys
from collections.abc import Mapping
from pathlib import Path

import yaml

# resolve_model 的结果字段，与 config.yaml 模型池一项的字段同名
_MODEL_ENV_SUFFIXES = ("PROVIDER", "API_BASE", "API_KEY", "MODEL", "CONTEXT_WINDOW", "REASONING")


def config_roots(
    config_dir: str | Path | None = None,
    xdg_config_home: str | Path | None = None,
) -> list[Path]:
    """候选状态根：显式 config_dir > PET_CONFIG_DIR > pet-dev（开发）> pet（安装版）。"""
    config_dir = config_dir or os.environ.get("PET_CONFIG_DIR")
    if config_dir:
        return [Path(config_dir)]
    if sys.platform == "darwin":
        base = Path.home() / "Library/Application Support"
    else:
        base = Path(
            xdg_config_home or os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config"
        )
    return [base / "pet-dev", base / "pet"]


def _active_model(root: Path) -> dict:
    """当前 Agent 引用的那条模型（config.yaml 的全局 models 池中的一项）。"""
    config = root / "config.yaml"
    if not config.exists():
        return {}
    settings = yaml.safe_load(config.read_text(encoding="utf-8")) or {}
    agents = settings.get("agents") or []
    active = settings.get("active_agent")
    agent = next((a for a in agents if a.get("id") == active), agents[0] if agents else {})
    return (settings.get("models") or {}).get(agent.get("model", "")) or {}


def model_env(prefix: str = "PET") -> dict[str, str]:
    """把 {prefix}_PROVIDER 等环境变量收成规范键；空串表示未设置。"""
    return {name.lower(): os.environ.get(f"{prefix}_{name}", "") for name in _MODEL_ENV_SUFFIXES}


def resolve_model(
    env: Mapping[str, str],
    *,
    model_override: str | None = None,
    config_dir: str | Path | None = None,
    xdg_config_home: str | Path | None = None,
    hint: str = "PET_API_BASE / PET_API_KEY / PET_MODEL",
) -> dict[str, str | int]:
    """env 优先、config.yaml 兜底；多个状态根里取第一个配了 api_key 的。"""
    found = [_active_model(root) for root in config_roots(config_dir, xdg_config_home)]
    cfg = next((m for m in found if m.get("api_key")), next((m for m in found if m), {}))
    api_base = env.get("api_base") or cfg.get("api_base", "")
    model = model_override or env.get("model") or cfg.get("model", "")
    if not api_base or not model:
        raise SystemExit(f"没有可用的模型配置：先在 GUI 里配好模型，或设 {hint}")
    return {
        "provider": env.get("provider") or cfg.get("provider") or "openai",
        "api_base": api_base,
        "api_key": env.get("api_key") or cfg.get("api_key", ""),
        "model": model,
        "context_window": int(env.get("context_window") or cfg.get("context_window") or 200_000),
        "reasoning": env.get("reasoning") or cfg.get("reasoning", ""),
    }


def mask_key(argv: list[str]) -> str:
    """打印命令行时把 PET_API_KEY=… 遮掉。"""
    return " ".join(c if not c.startswith("PET_API_KEY=") else "PET_API_KEY=***" for c in argv)