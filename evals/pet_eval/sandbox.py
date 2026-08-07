"""每条用例一个一次性的 PET_CONFIG_DIR。

布局（全部可丢弃）::

    <root>/config.yaml       只有一个 Agent，id 为 eval
    <root>/memory/eval/*.md  SOUL / USER / MEMORY / HEARTBEAT（来自 fixtures）
    <root>/skills/           空目录，主人真实的技能目录不会漏进来
    <root>/sessions/         pet-cli 落盘的会话
    <root>/logs/llm.log      每轮 LLM 请求一行 JSON
    <root>/work/             用例 prompt 指向的 workspace

隔离是重点：用例可以随便改写 MEMORY.md、删文件，碰不到主人真正的宠物，
而且每次运行都从完全相同的状态开始。
"""

from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path

import yaml
from pydantic import BaseModel

from .case import Case

# 沙箱里的 Agent id，也是记忆子目录名
AGENT_ID = "eval"
# 固定名字：用例行为不该跟着主人给宠物起的名字漂移
AGENT_NAME = "小宠"

MEMORY_FILES = ("SOUL.md", "USER.md", "MEMORY.md", "HEARTBEAT.md")


class ModelSpec(BaseModel):
    """评测跑在哪个模型上、怎么寻址。"""

    api_base: str
    api_key: str
    model: str
    context_window: int = 200_000
    reasoning_effort: str = ""
    thinking_enabled: bool = False
    thinking_budget_tokens: int = 4096


class Sandbox(BaseModel):
    root: Path
    work: Path
    # 播种之后的记忆内容快照，供 memory_written / memory_unchanged 比对
    memory_before: dict[str, str]

    @property
    def memory_dir(self) -> Path:
        return self.root / "memory" / AGENT_ID

    def read_work(self, rel: str) -> str | None:
        try:
            return (self.work / rel).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            return None

    def work_exists(self, rel: str) -> bool:
        return (self.work / rel).exists()

    def memory_changed(self) -> list[str]:
        after = _read_memory(self.memory_dir)
        return [name for name in MEMORY_FILES if after.get(name) != self.memory_before.get(name)]


def _read_memory(memory_dir: Path) -> dict[str, str]:
    out: dict[str, str] = {}
    for name in MEMORY_FILES:
        try:
            out[name] = (memory_dir / name).read_text(encoding="utf-8")
        except OSError:
            pass
    return out


def _config_yaml(root: Path, model: ModelSpec) -> dict:
    # AppSettings 的字段全都有 serde default，所以只写评测关心的这几项即可。
    return {
        "skills_dir": str(root / "skills"),
        "search_api_key": "",  # 没有 Tavily key ⇒ 不提供 web_search，工具集在每台机器上一致
        "active_agent": AGENT_ID,
        "agents": [
            {
                "id": AGENT_ID,
                "name": AGENT_NAME,
                "api_base": model.api_base,
                "api_key": model.api_key,
                "model": model.model,
                "context_window": model.context_window,
                "reasoning_effort": model.reasoning_effort,
                "thinking_enabled": model.thinking_enabled,
                "thinking_budget_tokens": model.thinking_budget_tokens,
            }
        ],
    }


def create(root: Path, case: Case, model: ModelSpec, fixtures: Path) -> Sandbox:
    """建好沙箱并播种。返回的 Sandbox 里已包含记忆快照。"""
    work = root / "work"
    for path in (root, work, root / "skills", root / "memory" / AGENT_ID):
        path.mkdir(parents=True, exist_ok=True)
    # config.yaml 里有真实 API key，别让同机其他用户读到
    os.chmod(root, 0o700)

    (root / "config.yaml").write_text(
        yaml.safe_dump(_config_yaml(root, model), allow_unicode=True, sort_keys=False),
        encoding="utf-8",
    )

    # 记忆基线走 fixtures 而不是 pet-core 的默认值：评测的起点要显式、可复现，
    # 也不该把主人真实的 SOUL.md 拖进来。
    memory_dir = root / "memory" / AGENT_ID
    for name in MEMORY_FILES:
        src = fixtures / "memory" / name
        if src.exists():
            shutil.copyfile(src, memory_dir / name)
    for name, content in case.memory.items():
        if name not in MEMORY_FILES:
            raise ValueError(f"{case.id}: 未知的记忆文件 {name}")
        (memory_dir / name).write_text(content, encoding="utf-8")

    for rel, content in case.seed.items():
        path = (work / rel).resolve()
        if not str(path).startswith(str(work.resolve())):
            raise ValueError(f"{case.id}: seed 路径逃出了 workspace：{rel}")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")

    for command in case.setup:
        done = subprocess.run(
            command, shell=True, cwd=work, capture_output=True, text=True, timeout=60
        )
        if done.returncode != 0:
            raise RuntimeError(f"setup `{command}` 失败：{done.stderr.strip()}")

    return Sandbox(root=root, work=work, memory_before=_read_memory(memory_dir))
