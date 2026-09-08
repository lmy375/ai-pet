"""eval-terminal-bench —— 用 Terminal-Bench 测 pet-cli 的终端任务能力。

Terminal-Bench 2.0 是 89 道 Harbor 格式的真实终端任务（编译、修 bug、数据处理、
运维…），每题一个 Docker 环境，跑完在同一容器里执行 tests/test.sh 判分。
这个入口把两件杂事串起来，然后把控制权交给 Harbor：

1. 把 terminal-bench-2 任务库 clone 到 vendor/（gitignored），checkout 到 Harbor 注册表
   里 terminal-bench@2.0 钉的那个 commit——等价于注册表数据集，但不经 Harbor Hub
   （公司 MITM 代理下 Hub 的 TLS 会被拒；git 走系统信任链没问题）；
2. 用 clux/muslrust 把 pet-cli 静态编译成 linux/amd64 二进制（缺了才编）；
3. ``harbor run -p vendor/terminal-bench-2 -a pet_tbench.agent:PetCliAgent``。

    uv run --project evals/eval-terminal-bench eval-terminal-bench --n-tasks 1     # 冒烟
    uv run --project evals/eval-terminal-bench eval-terminal-bench --only <task-name>
    uv run --project evals/eval-terminal-bench eval-terminal-bench                 # 全部 89 题（很重）
    uv run --project evals/eval-terminal-bench eval-terminal-bench --dataset terminal-bench@4.0.0  # 走 Hub

模型默认取主人真实 config.yaml 里当前 Agent 的（与 pet-eval 同规则），
PET_API_BASE / PET_API_KEY / PET_MODEL 可覆盖。需要本机 Docker。
结果落在 evals/eval-terminal-bench/runs/<job>/。
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

from pet_eval_common.binary import ensure_linux_binary
from pet_eval_common.model import mask_key, resolve_model

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
    parser.add_argument(
        "harbor_args", nargs="*", help="其余参数原样传给 harbor run（放在 -- 之后）"
    )
    args = parser.parse_args()

    model = resolve_model()
    if args.model:
        model["PET_MODEL"] = args.model
    binary = ensure_linux_binary(args.rebuild)

    if args.dataset:
        source_args = ["-d", args.dataset]
    else:
        path = Path(args.tasks_path) if args.tasks_path else ensure_tasks()
        if not path.exists():
            raise SystemExit(f"任务路径不存在：{path}")
        source_args = ["-p", str(path)]

    cmd = [
        "harbor", "run",
        *source_args,
        "-a", "pet_tbench.agent:PetCliAgent",
        "-o", str(RUNS),
        "-m", model["PET_MODEL"],
        "--ak", f"binary={binary}",
    ]
    # agent 类在 harbor 进程里读 env（含 --ae 注入的），容器里的 pet-cli 读 config.yaml
    for key, value in model.items():
        cmd += ["--ae", f"{key}={value}"]
    for key in OPTIONAL_ENV:
        if value := os.environ.get(key):
            cmd += ["--ae", f"{key}={value}"]
    for name in args.only or []:
        cmd += ["-i", name]
    for name in args.exclude or []:
        cmd += ["-x", name]
    if args.n_tasks:
        cmd += ["-l", str(args.n_tasks)]
    if args.attempts:
        cmd += ["-k", str(args.attempts)]
    if args.concurrency:
        cmd += ["-n", str(args.concurrency)]
    if args.timeout_multiplier:
        cmd += [
            "--timeout-multiplier", str(args.timeout_multiplier),
            "--ak", f"timeout_multiplier={args.timeout_multiplier}",
        ]
    cmd += args.harbor_args

    print(f"$ {mask_key(cmd)}\n")
    # 不往 posthog 报 job 统计（MITM 代理下还会白等 TLS 失败）
    done = subprocess.run(cmd, env={**os.environ, **model, "HARBOR_TELEMETRY": "0"})
    print(f"\n结果在 {RUNS}/<job>/（result.json、<trial>/verifier/reward.txt、agent/pet-cli.txt、agent/llm.log）")
    print(f"看轨迹：uv run --project {HERE.relative_to(REPO)} harbor view {RUNS}/<job>")
    sys.exit(done.returncode)
