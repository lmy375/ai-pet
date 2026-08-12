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
import os
import shutil
import subprocess
import sys
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parents[1]  # evals/eval-deep-swe
REPO = HERE.parents[1]
VENDOR = HERE / "vendor" / "deep-swe"
RUNS = HERE / "runs"
DEEP_SWE_GIT = "https://github.com/datacurve-ai/deep-swe"

MUSL_TARGET = "x86_64-unknown-linux-musl"
MUSL_BINARY = REPO / "target" / "musl" / MUSL_TARGET / "release" / "pet-cli"
# DeepSWE 的任务镜像是 amd64（swe-bench 系）；Apple Silicon 上 Docker 走 Rosetta
MUSL_BUILDER = "clux/muslrust:stable"


def resolve_model() -> dict[str, str]:
    """PET_API_* env 优先；缺的回落到主人真实 config.yaml 的当前 Agent。"""
    agent: dict = {}
    if sys.platform == "darwin":
        config = Path.home() / "Library/Application Support/pet/config.yaml"
    else:
        config = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "pet/config.yaml"
    if config.exists():
        settings = yaml.safe_load(config.read_text(encoding="utf-8")) or {}
        agents = settings.get("agents") or []
        active = settings.get("active_agent")
        agent = next((a for a in agents if a.get("id") == active), agents[0] if agents else {})

    resolved = {
        "PET_API_BASE": os.environ.get("PET_API_BASE") or agent.get("api_base", ""),
        "PET_API_KEY": os.environ.get("PET_API_KEY") or agent.get("api_key", ""),
        "PET_MODEL": os.environ.get("PET_MODEL") or agent.get("model", ""),
    }
    if not resolved["PET_API_BASE"] or not resolved["PET_MODEL"]:
        raise SystemExit(
            "没有可用的模型配置：先在 GUI 里配好 Agent，或设 PET_API_BASE / PET_API_KEY / PET_MODEL"
        )
    return resolved


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


def ensure_binary(rebuild: bool) -> Path:
    """Linux/amd64 静态 pet-cli。PET_CLI_LINUX_BIN 指现成的就不碰 Docker。"""
    if override := os.environ.get("PET_CLI_LINUX_BIN"):
        binary = Path(override)
        if not binary.exists():
            raise SystemExit(f"PET_CLI_LINUX_BIN 指向的文件不存在：{binary}")
        return binary
    if MUSL_BINARY.exists() and not rebuild:
        return MUSL_BINARY
    if not shutil.which("docker"):
        raise SystemExit("需要 Docker 来交叉编译 Linux 版 pet-cli（clux/muslrust）")
    print(f"用 {MUSL_BUILDER} 编译 {MUSL_TARGET} 版 pet-cli（首次较慢）…")
    # 下载在宿主机做（cargo fetch 信任系统代理/证书，公司 MITM 代理下容器内下载会挂），
    # 容器里 --offline 编译，宿主 registry 缓存挂到镜像的 CARGO_HOME（/opt/cargo）下。
    subprocess.run(["cargo", "fetch", "--target", MUSL_TARGET], cwd=REPO, check=True)
    done = subprocess.run(
        [
            "docker", "run", "--rm", "--platform", "linux/amd64",
            "-v", f"{REPO}:/volume", "-w", "/volume",
            "-v", f"{Path.home()}/.cargo/registry:/opt/cargo/registry",
            "-e", "CARGO_TARGET_DIR=/volume/target/musl",
            MUSL_BUILDER,
            "cargo", "build", "--release", "-p", "pet-cli",
            "--target", MUSL_TARGET, "--offline",
        ]
    )
    if done.returncode != 0 or not MUSL_BINARY.exists():
        raise SystemExit("musl 交叉编译失败")
    return MUSL_BINARY


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
    binary = ensure_binary(args.rebuild)

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

    shown = " ".join(c if not c.startswith("PET_API_KEY=") else "PET_API_KEY=***" for c in cmd)
    print(f"$ {shown}\n")
    done = subprocess.run(cmd)
    print(f"\n结果在 {RUNS}/<job>/（reward.json、agent/pet-cli.txt、agent/llm.log）")
    print(f"看轨迹：uv run --project {HERE.relative_to(REPO)} pier view {RUNS}/<job>")
    sys.exit(done.returncode)
