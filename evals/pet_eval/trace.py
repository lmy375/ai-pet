"""从沙箱的 llm.log 还原这次运行的轨迹。

引擎每发一次 LLM 请求就往 llm.log 追加一行 JSON，里面有完整的 request 和
response（含 tool_calls）。沙箱是全新的，所以这个文件里的内容恰好就是这条用例
的全部——不需要另外埋点，也不用去解析终端输出。
"""

from __future__ import annotations

import json
from pathlib import Path

from pydantic import BaseModel, Field


class Call(BaseModel):
    name: str
    arguments: str


class Trace(BaseModel):
    # 顶层对话的轮数。子代理跑的是独立会话（log_session 带 ":sub:"），不计入。
    rounds: int = 0
    # 所有工具调用，按发生顺序。子代理内部的调用也算——「有没有人用 sed 改文件」
    # 这种问题，不该因为活是外包出去的就放过。
    calls: list[Call] = Field(default_factory=list)
    # 最后一轮的可见回复，不做断言，但用例挂掉时第一个想看的就是它
    text: str = ""

    def names(self) -> list[str]:
        return [c.name for c in self.calls]

    def count(self, name: str) -> int:
        return sum(1 for c in self.calls if c.name == name)

    def bash_commands(self) -> list[str]:
        out = []
        for call in self.calls:
            if call.name != "bash":
                continue
            try:
                out.append(json.loads(call.arguments).get("command", ""))
            except json.JSONDecodeError:
                out.append(call.arguments)
        return out


def parse(llm_log: Path) -> Trace:
    trace = Trace()
    try:
        lines = llm_log.read_text(encoding="utf-8").splitlines()
    except OSError:
        return trace

    for line in lines:
        if not line.strip():
            continue
        try:
            entry = json.loads(line)
        except json.JSONDecodeError:
            continue
        if ":sub:" not in str(entry.get("session_id", "")):
            trace.rounds += 1
        response = entry.get("response") or {}
        for call in response.get("tool_calls") or []:
            function = call.get("function") or {}
            trace.calls.append(
                Call(name=function.get("name", ""), arguments=function.get("arguments", "") or "")
            )
        if response.get("text"):
            trace.text = response["text"]
    return trace
