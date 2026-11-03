"""两条容器评测链路共用的 LLM prompt：只有「是否要求 commit」不同。

DeepSWE 的 verifier 从 git 提交里收 patch，必须 commit；Terminal-Bench 的 verifier
检查容器磁盘的最终状态，commit 会让改动离开任务点名的位置。其余说明完全一致，
所以模板只留一份，差异由 ``require_commit`` 决定。
"""

from __future__ import annotations

from pathlib import Path

from jinja2 import Environment, StrictUndefined

PROMPT_TEMPLATE = Path(__file__).with_name("prompt.j2")

_TEMPLATE = Environment(
    undefined=StrictUndefined, trim_blocks=True, lstrip_blocks=True
).from_string(PROMPT_TEMPLATE.read_text(encoding="utf-8"))


def render_prompt(instruction: str, *, require_commit: bool) -> str:
    """把任务 instruction 套进 harness 的固定说明。"""
    return _TEMPLATE.render(instruction=instruction, require_commit=require_commit)