"""pet-eval —— 宠物到底有没有把事做完，以及是不是按交代的方式做的。

一条用例就是一个 YAML：prompt、workspace 初始状态、做完之后必须成立的事（磁盘文件、
工具轨迹、轮数）。跑法是真的 ``pet-cli -p`` 打一轮，PET_CONFIG_DIR 指向一次性目录，
跑完从沙箱的 llm.log 还原轨迹再做断言。

    uv run --project evals/eval-private pet-eval --repeat 3
    uv run --project evals/eval-private pet-eval --only edit --model GPT-5.5

注意：用例会在这台机器上真的执行工具（含 bash），把一次运行当成在本地跑 agent。
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
import time
from datetime import datetime
from pathlib import Path

from pet_eval_common.model import resolve_model
from pydantic import BaseModel, Field, computed_field

from . import checks, sandbox, trace
from .case import WORK, Case, load_dir
from .sandbox import ModelSpec, Sandbox
from .settings import EvalSettings

EVALS = Path(__file__).resolve().parents[1]
REPO = EVALS.parents[1]

GREEN, RED, YELLOW, DIM, RESET = "\033[32m", "\033[31m", "\033[33m", "\033[2m", "\033[0m"
if not sys.stdout.isatty():
    GREEN = RED = YELLOW = DIM = RESET = ""


def resolve_spec(env: EvalSettings, override: str | None) -> ModelSpec:
    """默认跑主人实际在用的那个 Agent 的模型，PET_EVAL_* 和 --model 可以覆盖。"""
    merged = resolve_model(
        env.model_env(),
        model_override=override,
        config_dir=env.config_dir,
        xdg_config_home=env.xdg_config_home,
        hint="PET_EVAL_API_BASE / PET_EVAL_MODEL",
    )
    return ModelSpec(**merged)


def resolve_binary(env: EvalSettings) -> Path:
    """找到 pet-cli；没有就现编一个 debug 版。"""
    if env.cli_bin is not None:
        return env.cli_bin
    for profile in ("release", "debug"):
        candidate = REPO / "target" / profile / "pet-cli"
        if candidate.exists():
            return candidate
    print(f"{DIM}pet-cli 还没编译，先 cargo build -p pet-cli …{RESET}")
    if subprocess.run(["cargo", "build", "-p", "pet-cli"], cwd=REPO).returncode != 0:
        raise SystemExit("cargo build -p pet-cli 失败")
    return REPO / "target/debug/pet-cli"


class Attempt(BaseModel):
    passed: bool
    failures: list[str]
    rounds: int
    tools: list[str]
    elapsed_s: float
    sandbox: Path
    # 不做断言，但用例挂掉时第一个想看的就是它
    reply: str


class CaseResult(BaseModel):
    id: str
    axis: str
    attempts: list[Attempt] = Field(default_factory=list)

    @computed_field
    @property
    def passed(self) -> int:
        return sum(1 for a in self.attempts if a.passed)

    @computed_field
    @property
    def all_passed(self) -> bool:
        return self.passed == len(self.attempts)

    @computed_field
    @property
    def flaky(self) -> bool:
        return 0 < self.passed < len(self.attempts)


class Report(BaseModel):
    started_at: datetime
    model: str
    api_base: str
    repeat: int
    sandbox_root: Path
    cases: list[CaseResult]


def run_attempt(case: Case, model: ModelSpec, binary: Path, root: Path, timeout: int) -> Attempt:
    started = time.monotonic()
    errors: list[str] = []

    try:
        box: Sandbox = sandbox.create(root, case, model, EVALS / "fixtures")
    except Exception as exc:  # 沙箱都没搭起来，直接判失败
        return Attempt(
            passed=False,
            failures=[f"沙箱准备失败：{exc}"],
            rounds=0,
            tools=[],
            elapsed_s=round(time.monotonic() - started, 1),
            sandbox=root,
            reply="",
        )

    prompt = case.prompt.replace(WORK, str(box.work))
    try:
        done = subprocess.run(
            [str(binary), "-p", prompt],
            cwd=box.work,
            env={**os.environ, "PET_CONFIG_DIR": str(root)},
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        (root / "stdout.txt").write_text(done.stdout, encoding="utf-8")
        (root / "stderr.txt").write_text(done.stderr, encoding="utf-8")
        if done.returncode != 0:
            errors.append(f"pet-cli 退出码 {done.returncode}：{done.stderr.strip()[:300]}")
    except subprocess.TimeoutExpired:
        errors.append(f"超过 {timeout}s 未结束")

    run_trace = trace.parse(root / "logs/llm.log")
    failures = checks.run(case, box, run_trace, errors)
    return Attempt(
        passed=not failures,
        failures=failures,
        rounds=run_trace.rounds,
        tools=run_trace.names(),
        elapsed_s=round(time.monotonic() - started, 1),
        sandbox=root,
        reply=run_trace.text,
    )


def print_case(result: CaseResult, repeat: int) -> None:
    mark = (
        f"{GREEN}✓{RESET}"
        if result.all_passed
        else f"{YELLOW}~{RESET}" if result.flaky else f"{RED}✗{RESET}"
    )
    score = f" {result.passed}/{len(result.attempts)}" if repeat > 1 else ""
    last = result.attempts[-1]
    print(f"  {mark} {result.id:<32}{score} {DIM}{last.rounds} 轮  {last.elapsed_s}s{RESET}")
    for attempt in result.attempts:
        if attempt.passed:
            continue
        for failure in attempt.failures:
            print(f"      {RED}·{RESET} {failure}")
        print(f"      {DIM}sandbox: {attempt.sandbox}{RESET}")


def main() -> None:
    parser = argparse.ArgumentParser(prog="pet-eval", description="宠物 Agent 行为评测")
    parser.add_argument("--cases", type=Path, default=EVALS / "cases", help="用例目录")
    parser.add_argument("--only", help="只跑 id 含该子串的用例")
    parser.add_argument("--repeat", type=int, default=1, help="每条用例跑几次（默认 1）")
    parser.add_argument("--model", help="覆盖模型（默认用 config.yaml 当前 Agent 的）")
    parser.add_argument(
        "--out", type=Path, help="结果 JSON（默认 evals/eval-private/runs/<时间戳>.json）"
    )
    parser.add_argument("--timeout", type=int, default=300, help="单次运行超时秒数")
    args = parser.parse_args()

    if args.repeat < 1:
        raise SystemExit("--repeat 至少为 1")

    cases = load_dir(args.cases, args.only)
    if not cases:
        raise SystemExit(f"{args.cases} 下没有匹配的用例")

    env = EvalSettings()
    model = resolve_spec(env, args.model)
    binary = resolve_binary(env)
    stamp = datetime.now().strftime("%Y%m%dT%H%M%S")
    sandbox_root = Path(tempfile.gettempdir()) / "pet-eval" / stamp

    print(f"{len(cases)} cases · model {model.model} · repeat {args.repeat}\n")

    results: list[CaseResult] = []
    axis = None
    for case in cases:
        if case.axis != axis:
            axis = case.axis
            print(axis)
        result = CaseResult(id=case.id, axis=case.axis)
        for n in range(args.repeat):
            name = f"{case.id}-{n + 1}" if args.repeat > 1 else case.id
            result.attempts.append(
                run_attempt(case, model, binary, sandbox_root / name, args.timeout)
            )
        print_case(result, args.repeat)
        results.append(result)

    passed = sum(1 for r in results if r.all_passed)
    flaky = sum(1 for r in results if r.flaky)
    summary = f"\n{passed}/{len(results)} passed"
    if flaky:
        summary += f" · {YELLOW}{flaky} flaky{RESET}"
    for axis in dict.fromkeys(r.axis for r in results):
        in_axis = [r for r in results if r.axis == axis]
        summary += f" · {axis} {sum(1 for r in in_axis if r.all_passed)}/{len(in_axis)}"
    print(summary)

    report = Report(
        started_at=datetime.now(),
        model=model.model,
        api_base=model.api_base,
        repeat=args.repeat,
        sandbox_root=sandbox_root,
        cases=results,
    )
    out = args.out or EVALS / "runs" / f"{stamp}.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(report.model_dump_json(indent=2), encoding="utf-8")
    print(f"{DIM}report: {out}{RESET}")
    print(f"{DIM}sandboxes: {sandbox_root}{RESET}")
    sys.exit(0 if passed == len(results) else 1)
