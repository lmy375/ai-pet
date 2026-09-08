"""Linux/amd64 静态 pet-cli：找现成的，或用 clux/muslrust 交叉编译。"""

from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]

MUSL_TARGET = "x86_64-unknown-linux-musl"
MUSL_BINARY = REPO / "target" / "musl" / MUSL_TARGET / "release" / "pet-cli"
# 任务镜像都是 amd64（swe-bench 系 / Terminal-Bench）；Apple Silicon 上 Docker 走 Rosetta
MUSL_BUILDER = "clux/muslrust:stable"


def ensure_linux_binary(rebuild: bool = False) -> Path:
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


def resolve_host_binary(override: str | None, build_hint: str) -> Path:
    """agent 进程里定位要上传的二进制：--ak binary=… > PET_CLI_LINUX_BIN > 默认产物。"""
    raw = override or os.environ.get("PET_CLI_LINUX_BIN")
    binary = Path(raw) if raw else MUSL_BINARY
    if not binary.exists():
        raise FileNotFoundError(
            f"找不到 Linux 版 pet-cli：{binary}\n"
            f"先用 {build_hint} 构建（docker + clux/muslrust），"
            "或设 PET_CLI_LINUX_BIN 指向现成的二进制。"
        )
    return binary
