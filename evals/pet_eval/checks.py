"""把用例的 expect 变成一串失败原因。

这里全是确定性判断——磁盘状态和工具轨迹。Stage 1 没有模型裁判：失败是事实不是
观点，红一条就意味着真的有东西变了。
"""

from __future__ import annotations

from .case import Case
from .sandbox import Sandbox
from .trace import Trace


def run(case: Case, sandbox: Sandbox, trace: Trace, errors: list[str]) -> list[str]:
    """每条被违反的期望产出一行；空列表 = 通过。"""
    failures = list(errors)
    _files(case, sandbox, failures)
    _tools(case, trace, failures)
    _memory(case, sandbox, failures)

    max_rounds = case.expect.max_rounds
    if max_rounds is not None and trace.rounds > max_rounds:
        failures.append(f"跑了 {trace.rounds} 轮 LLM，最多允许 {max_rounds} 轮")
    return failures


def _files(case: Case, sandbox: Sandbox, failures: list[str]) -> None:
    for rel, expect in case.expect.files.items():
        if not expect.exists:
            if sandbox.work_exists(rel):
                failures.append(f"{rel}: 还在，应该已经被删掉")
            continue
        content = sandbox.read_work(rel)
        if content is None:
            failures.append(f"{rel}: 不存在（或不是可读文本）")
            continue
        for needle in expect.contains:
            if needle not in content:
                failures.append(f"{rel}: 少了期望内容 {needle!r}")
        for needle in expect.not_contains:
            if needle in content:
                failures.append(f"{rel}: 仍然包含 {needle!r}")


def _tools(case: Case, trace: Trace, failures: list[str]) -> None:
    expect = case.expect.tools
    used = trace.names()

    for name in expect.must_use:
        if name not in used:
            failures.append(f"没有调用 {name}（实际调用：{', '.join(used) or '无'}）")
    for name in expect.must_not_use:
        if name in used:
            failures.append(f"调用了 {name}，本用例不允许")
    if expect.first_use:
        if not used:
            failures.append(f"一个工具都没调用，期望第一个是 {expect.first_use}")
        elif used[0] != expect.first_use:
            failures.append(f"第一个工具是 {used[0]}，期望 {expect.first_use}")
    for name, want in expect.counts.items():
        got = trace.count(name)
        if got != want:
            failures.append(f"{name} 调用了 {got} 次，期望 {want} 次")
    for pattern in expect.bash_must_not_match:
        for command in trace.bash_commands():
            if pattern in command:
                failures.append(f"bash 命令里出现了 {pattern!r}：{command}")


def _memory(case: Case, sandbox: Sandbox, failures: list[str]) -> None:
    if not case.expect.memory_written and not case.expect.memory_unchanged:
        return
    changed = sandbox.memory_changed()
    for name in case.expect.memory_written:
        if name not in changed:
            failures.append(f"{name} 没有被更新")
    for name in case.expect.memory_unchanged:
        if name in changed:
            failures.append(f"{name} 被改了，本不该动它")
