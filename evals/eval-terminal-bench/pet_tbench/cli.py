"""eval-terminal-bench —— 用 Terminal-Bench 测 pet-cli 的终端任务能力。

入口把任务库钉到 Harbor 注册表用的 commit（不经 Hub，避免公司 MITM 代理下 TLS 被拒），
然后把控制权交给 harbor：
    uv run --project evals/eval-terminal-bench eval-terminal-bench --n-tasks 1   # 冒烟
    uv run --project evals/eval-terminal-bench eval-terminal-bench --only <name>
    uv run --project evals/eval-terminal-bench eval-terminal-bench               # 全部 89 题（很重）

模型默认取主人真实 config.yaml 里当前 Agent 的，PET_API_BASE / PET_API_KEY / PET_MODEL
可覆盖。需要本机 Docker；结果落在 runs/<job>/。
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

from pet_eval_common.host import launch

HERE = Path(__file__).resolve().parents[1]  # evals/eval-terminal-bench
REPO = HERE.parents[1]
RUNS = HERE / "runs"
VENDOR = HERE / "vendor" / "terminal-bench-2"
# Harbor registry.json 里 terminal-bench@2.0 的 89 道题全部指向这个 repo 的这个 commit
TB2_GIT = "https://github.com/laude-institute/terminal-bench-2.git"
TB2_COMMIT = "69671fbaac6d67a7ef0dfec016cc38a64ef7a77c"

# 有值就一并带进容器的可选模型参数
OPTIONAL_ENV = ("PET_CONTEXT_WINDOW", "PET_REASONING")


def ensure_tasks() -> Path:
    """vendor/ 下钉在 TB2_COMMIT 的任务库；已存在且 HEAD 对得上就不碰网络。"""
    if not VENDOR.exists():
        print(f"clone {TB2_GIT} → {VENDOR} …")
        VENDOR.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "clone", "-q", "--filter=blob:none", TB2_GIT, str(VENDOR)], check=True)
    head = subprocess.run(
        ["git", "-C", str(VENDOR), "rev-parse", "HEAD"], capture_output=True, text=True, check=True
    ).stdout.strip()
    if head != TB2_COMMIT:
        subprocess.run(["git", "-C", str(VENDOR), "fetch", "-q", "origin", TB2_COMMIT], check=True)
        subprocess.run(["git", "-C", str(VENDOR), "checkout", "-q", TB2_COMMIT], check=True)
    return VENDOR


def main() -> None:
    parser = argparse.ArgumentParser(
        prog="eval-terminal-bench",
        description="Terminal-Bench 跑 pet-cli（Harbor 驱动，需要 Docker）",
    )
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--dataset", help="改跑 Harbor Hub 上的数据集（如 terminal-bench@4.0.0；需要能直连 Hub）")
    source.add_argument("--tasks-path", help="改跑本地 Harbor 格式任务目录（单题或整个数据集）")
    parser.add_argument("--only", action="append", help="只跑名字匹配的题（glob，可重复）")
    parser.add_argument("--exclude", action="append", help="排除名字匹配的题（glob，可重复）")
    parser.add_argument("--n-tasks", type=int, help="最多跑 N 题（过滤之后取前 N）")
    parser.add_argument("--attempts", type=int, help="每题跑 K 次（pass@k）")
    parser.add_argument("--concurrency", type=int, help="并发 trial 数（Harbor 默认 4）")
    parser.add_argument("--timeout-multiplier", type=float, help="任务超时倍率（同时告知 agent，预算随之放大）")
    parser.add_argument("--model", help="覆盖模型（默认用 config.yaml 当前 Agent 的）")
    parser.add_argument("--rebuild", action="store_true", help="强制重编 Linux pet-cli")
    parser.add_argument("harbor_args", nargs="*", help="其余参数原样传给 harbor run（放在 -- 之后）")
    args = parser.parse_args()

    if args.dataset:
        source_args = ["-d", args.dataset]
    else:
        path = Path(args.tasks_path) if args.tasks_path else ensure_tasks()
        if not path.exists():
            raise SystemExit(f"任务路径不存在：{path}")
        source_args = ["-p", str(path)]

    extra_args: list[str] = []
    for key in OPTIONAL_ENV:
        if value := os.environ.get(key):
            extra_args += ["--ae", f"{key}={value}"]
    for name in args.only or []:
        extra_args += ["-i", name]
    for name in args.exclude or []:
        extra_args += ["-x", name]
    if args.n_tasks:
        extra_args += ["-l", str(args.n_tasks)]
    if args.attempts:
        extra_args += ["-k", str(args.attempts)]
    if args.concurrency:
        extra_args += ["-n", str(args.concurrency)]
    if args.timeout_multiplier:
        extra_args += [
            "--timeout-multiplier", str(args.timeout_multiplier),
            "--ak", f"timeout_multiplier={args.timeout_multiplier}",
        ]
    extra_args += args.harbor_args

    sys.exit(
        launch(
            ["harbor", "run", *source_args, "-a", "pet_tbench.agent:PetCliAgent", "-o", str(RUNS)],
            model_override=args.model,
            rebuild=args.rebuild,
            model_hint=(
                "PET_API_BASE / PET_API_KEY / PET_MODEL"
                "（宿主 env 或 harbor run --ae；model 也可用 --model 传）"
            ),
            extra_args=extra_args,
            # 不往 posthog 报 job 统计（MITM 代理下还会白等 TLS 失败）
            env={"HARBOR_TELEMETRY": "0"},

            summary=(
                f"结果在 {RUNS}/<job>/"
                "（result.json、<trial>/verifier/reward.txt、agent/pet-cli.txt、agent/llm.log）"
            ),
            view_command=f"{HERE.relative_to(REPO)} harbor view {RUNS}/<job>",
        )
    )