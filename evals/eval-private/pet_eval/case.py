"""用例文件格式：evals/eval-private/cases/ 下一个 YAML 一条用例。

所有模型都 `extra="forbid"`——期望里写错一个 key 会直接报错，而不是静默不检查、
用例照样绿。那是评测集烂掉的经典方式。
"""

from __future__ import annotations

from pathlib import Path

import yaml
from pydantic import BaseModel, ConfigDict, Field, ValidationError

# 发出前会被替换成沙箱 workspace 的绝对路径。用例里提到的所有路径都要用它——
# 工具收的是绝对路径，而 workspace 每次运行都在新的临时目录里。
WORK = "{WORK}"


class Strict(BaseModel):
    """用例里的结构一律不接受未知字段。"""

    model_config = ConfigDict(extra="forbid")


class FileExpect(Strict):
    """对 workspace 里某个文件的期望。"""

    # False = 断言它已经被删掉
    exists: bool = True
    contains: list[str] = Field(default_factory=list)
    not_contains: list[str] = Field(default_factory=list)


class ToolExpect(Strict):
    must_use: list[str] = Field(default_factory=list)
    must_not_use: list[str] = Field(default_factory=list)
    # 第一个动作是什么（例如改文件前必须先 read_file）
    first_use: str | None = None
    # 精确调用次数。配合 max_rounds 就能钉住「无依赖的调用要并行发」：
    # 两次调用、至多两轮，只可能是同一轮里一起发出的。
    counts: dict[str, int] = Field(default_factory=dict)
    # 不允许出现在任何 bash 命令里的子串（选错工具、或危险命令）
    bash_must_not_match: list[str] = Field(default_factory=list)


class Expect(Strict):
    files: dict[str, FileExpect] = Field(default_factory=dict)
    tools: ToolExpect = Field(default_factory=ToolExpect)
    # 必须被改写的记忆文件（例如定时类需求要落到 HEARTBEAT.md）
    memory_written: list[str] = Field(default_factory=list)
    # 必须没被动过的记忆文件——这条才是抓「什么都往 MEMORY.md 塞」的
    memory_unchanged: list[str] = Field(default_factory=list)
    # LLM 轮数上限（一轮 = 工具循环里的一次请求）
    max_rounds: int | None = None


class Case(Strict):
    id: str
    # 汇总时按这个分组（tool-selection / task-completion / safety …）
    axis: str
    # 发给 Agent 的那一句
    prompt: str
    # 发出前在 workspace 里跑的 shell 命令，用于文件内容表达不了的状态（如 git init）
    setup: list[str] = Field(default_factory=list)
    # 写进 workspace 的初始文件：相对路径 -> 内容
    seed: dict[str, str] = Field(default_factory=dict)
    # 覆盖该用例的记忆文件（SOUL.md / USER.md / MEMORY.md / HEARTBEAT.md）
    memory: dict[str, str] = Field(default_factory=dict)
    expect: Expect = Field(default_factory=Expect)


def load_dir(cases_dir: Path, only: str | None = None) -> list[Case]:
    """读取目录下所有 *.yaml，按 id 排序；only 给定时只保留 id 含该子串的。"""
    cases: list[Case] = []
    for path in sorted(cases_dir.glob("*.yaml")):
        try:
            case = Case.model_validate(yaml.safe_load(path.read_text(encoding="utf-8")))
        except ValidationError as exc:
            raise SystemExit(f"{path.name} 用例格式有问题：\n{exc}") from exc
        if only and only not in case.id:
            continue
        cases.append(case)
    cases.sort(key=lambda c: c.id)
    return cases
