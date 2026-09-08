"""评测用哪只模型：PET_* env 优先，缺的回落到主人真实 config.yaml 的当前 Agent。"""

from __future__ import annotations

import os
import sys
from pathlib import Path

import yaml

MODEL_ENV_KEYS = ("PET_PROVIDER", "PET_API_BASE", "PET_API_KEY", "PET_MODEL")


def _config_roots() -> list[Path]:
    """候选状态根，按优先级：PET_CONFIG_DIR > pet-dev（`pnpm app` 用的）> pet（安装版）。"""
    if override := os.environ.get("PET_CONFIG_DIR"):
        return [Path(override)]
    if sys.platform == "darwin":
        base = Path.home() / "Library/Application Support"
    else:
        base = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")
    return [base / "pet-dev", base / "pet"]


def _active_agent(root: Path) -> dict:
    config = root / "config.yaml"
    if not config.exists():
        return {}
    settings = yaml.safe_load(config.read_text(encoding="utf-8")) or {}
    agents = settings.get("agents") or []
    active = settings.get("active_agent")
    return next((a for a in agents if a.get("id") == active), agents[0] if agents else {})


def resolve_model() -> dict[str, str]:
    """返回 {PET_PROVIDER, PET_API_BASE, PET_API_KEY, PET_MODEL}；没有可用配置直接 SystemExit。

    多个状态根里取第一个配了 api_key 的当前 Agent（安装版默认 Agent 往往是空壳）。
    """
    agents = [_active_agent(root) for root in _config_roots()]
    agent = next((a for a in agents if a.get("api_key")), next((a for a in agents if a), {}))

    resolved = {
        "PET_PROVIDER": os.environ.get("PET_PROVIDER") or agent.get("provider") or "openai",
        "PET_API_BASE": os.environ.get("PET_API_BASE") or agent.get("api_base", ""),
        "PET_API_KEY": os.environ.get("PET_API_KEY") or agent.get("api_key", ""),
        "PET_MODEL": os.environ.get("PET_MODEL") or agent.get("model", ""),
    }
    if not resolved["PET_API_BASE"] or not resolved["PET_MODEL"]:
        raise SystemExit(
            "没有可用的模型配置：先在 GUI 里配好 Agent，或设 PET_API_BASE / PET_API_KEY / PET_MODEL"
        )
    return resolved


def mask_key(argv: list[str]) -> str:
    """打印命令行时把 PET_API_KEY=… 遮掉。"""
    return " ".join(c if not c.startswith("PET_API_KEY=") else "PET_API_KEY=***" for c in argv)
