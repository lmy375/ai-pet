"""eval-deep-swe —— 用 DeepSWE benchmark 测 pet-cli 的 coding agent 能力。

DeepSWE（datacurve-ai/deep-swe）是 113 道 Harbor 格式的真实工程任务，每题一个
Docker 隔离环境，verifier 从 git 提交里收 patch 自动判分。这个入口把三件杂事
串起来，然后把控制权交给 pier：

1. clone/更新 deep-swe 任务库到 vendor/（gitignored）；
2. 用 clux/muslrust 把 pet-cli 静态编译成 linux/amd64 二进制（缺了才编）；
3. ``pier run -p <tasks> --agent-import-path pet_deep_swe.agent:PetCliAgent``。

    uv run --project evals/eval-deep-swe eval-deep-swe --n-tasks 1   # 冒烟
    uv run --project evals/eval-deep-swe eval-deep-swe --only <task-id>
    uv run --project evals/eval-deep-swe eval-deep-swe               # 全部 113 题（很重）

模型默认取主人真实 config.yaml 里当前 Agent 的（与 pet-eval 同规则），
PET_API_BASE / PET_API_KEY / PET_MODEL 可覆盖。需要本机 Docker。
结果落在 evals/eval-deep-swe/runs/<job>/，用 ``pier view <job目录>`` 看轨迹。
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

from pet_eval_common.binary import ensure_linux_binary
from pet_eval_common.model import mask_key, resolve_model

HERE = Path(__file__).resolve().parents[1]  # evals/eval-deep-swe
REPO = HERE.parents[1]
VENDOR = HERE / "vendor" / "deep-swe"
RUNS = HERE / "runs"
DEEP_SWE_GIT = "https://github.com/datacurve-ai/deep-swe"

def ensure_tasks(update: bool) -> Path:
    if not VENDOR.exists():
        print(f"clone {DEEP_SWE_GIT} → {VENDOR} …")
        VENDOR.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            ["git", "clone", "--depth", "1", DEEP_SWE_GIT, str(VENDOR)], check=True
        )
    elif update:
        subprocess.run(["git", "-C", str(VENDOR), "pull", "--ff-only"], check=True)
    return VENDOR / "tasks"


def main() -> None:
    parser = argparse.ArgumentParser(
        prog="eval-deep-swe", description="DeepSWE benchmark 跑 pet-cli（pier 驱动，需要 Docker）"
    )
    parser.add_argument("--only", help="只跑这一道题（tasks/ 下的目录名）")
    parser.add_argument("--n-tasks", type=int, help="抽样跑 N 题")
    parser.add_argument("--sample-seed", type=int, default=0, help="抽样种子（默认 0）")
    parser.add_argument("--model", help="覆盖模型（默认用 config.yaml 当前 Agent 的）")
    parser.add_argument("--rebuild", action="store_true", help="强制重编 Linux pet-cli")
    parser.add_argument("--update-tasks", action="store_true", help="git pull 更新 deep-swe 任务库")
    parser.add_argument(
        "pier_args", nargs="*", help="其余参数原样传给 pier run（放在 -- 之后）"
    )
    args = parser.parse_args()

    model = resolve_model()
    if args.model:
        model["PET_MODEL"] = args.model
    tasks = ensure_tasks(args.update_tasks)
    binary = ensure_linux_binary(args.rebuild)

    path = tasks / args.only if args.only else tasks
    if not path.exists():
        raise SystemExit(f"任务路径不存在：{path}")

    cmd = [
        "pier", "run",
        "-p", str(path),
        "--agent-import-path", "pet_deep_swe.agent:PetCliAgent",
        "--jobs-dir", str(RUNS),
        "-m", model["PET_MODEL"],
        "--ak", f"binary={binary}",
    ]
    for key, value in model.items():
        cmd += ["--ae", f"{key}={value}"]
    if args.n_tasks:
        cmd += ["--n-tasks", str(args.n_tasks), "--sample-seed", str(args.sample_seed)]
    cmd += args.pier_args

    print(f"$ {mask_key(cmd)}\n")
    done = subprocess.run(cmd)
    print(f"\n结果在 {RUNS}/<job>/（reward.json、agent/pet-cli.txt、agent/llm.log）")
    print(f"看轨迹：uv run --project {HERE.relative_to(REPO)} pier view {RUNS}/<job>")
    sys.exit(done.returncode)
