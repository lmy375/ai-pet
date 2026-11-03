"""Docker 类评测宿主侧公共骨架：config.yaml、安装、one-shot wrapper、CLI 尾巴。

pier/Harbor 的真实差异（install() vs install_spec()、超时预算、SOUL/commit）留在各自包内。
"""

from __future__ import annotations

import os
import shlex
import subprocess
from pathlib import Path
from typing import Protocol

from .binary import ensure_linux_binary, resolve_host_binary
from .container import BINARY, CONFIG_DIR, EXTRA_CA_PEM, ca_setup_cmds, config_dir_setup_cmds, config_yaml, count_llm_rounds
from .model import mask_key, model_env, resolve_model

# 下发到容器 agent 进程的模型 env；context_window/reasoning 由各 CLI 决定是否带
_MODEL_AE_KEYS = ("provider", "api_base", "api_key", "model")


class HostEnvironment(Protocol):
    async def upload_file(self, source: Path, target: str) -> None: ...


class PetCliHostAgent:
    """两个 BaseInstalledAgent 共用的 _config_yaml 与安装尾巴。"""

    CONFIG_HINT = ""

    def _config_yaml(self) -> str:
        return config_yaml(
            provider=self._get_env("PET_PROVIDER") or "openai",
            api_base=self._get_env("PET_API_BASE") or "",
            api_key=self._get_env("PET_API_KEY") or "",
            model=self._get_env("PET_MODEL") or self.model_name or "",
            context_window=int(self._get_env("PET_CONTEXT_WINDOW") or 200_000),
            reasoning=self._get_env("PET_REASONING") or "",
            hint=self.CONFIG_HINT,
        )

    async def _install_runtime(
        self, environment: HostEnvironment, memory_files: dict[str, str]
    ) -> None:
        """上传二进制、搭 PET_CONFIG_DIR、可选注入 CA。"""
        await environment.upload_file(
            resolve_host_binary(self._binary_override, self._build_hint), BINARY
        )
        cmds = [config_dir_setup_cmds(self._config_yaml(), memory_files)]
        if self._extra_ca:
            await environment.upload_file(self._extra_ca, EXTRA_CA_PEM)
            cmds.append(ca_setup_cmds())
        await self.exec_as_root(environment, command="\n".join(cmds))


def memory_baseline(benchmark: str, soul: str) -> dict[str, str]:
    """记忆基线；SOUL 由各 benchmark 提供（commit 语义相反，不能合并）。"""
    return {
        "SOUL.md": soul,
        "USER.md": f"# 关于主人\n\n（{benchmark} 评测：无主人信息。专注完成任务本身。）\n",
        "MEMORY.md": f"# 我的记忆\n\n（{benchmark} 评测基线：刻意留空。）\n",
        "HEARTBEAT.md": f"# 定时任务\n\n（{benchmark} 评测基线：无任务。）\n",
    }


def round_stats(logs_dir: Path) -> tuple[int, int]:
    """llm.log 不含 token usage，只能给出 (轮数, 工具调用数)。"""
    return count_llm_rounds(logs_dir / "llm.log")


def run_script(
    instruction: str, *, budget_sec: int | None = None, auto_commit: bool = False
) -> str:
    """one-shot wrapper：导出 llm.log/sessions，永远 exit 0；budget_sec 用 timeout 兜底。"""
    run = f"{BINARY} -p {shlex.quote(instruction)} </dev/null 2>&1 | tee -a /logs/agent/pet-cli.txt || rc=$?"
    if budget_sec is not None:
        run = f"""echo "budget={budget_sec}s" > /logs/agent/pet-cli.txt
if command -v timeout >/dev/null 2>&1; then
  timeout --foreground {budget_sec}s {run}
else
  {run}
fi"""
    commit = "" if not auto_commit else """
cd /app
if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
  git checkout -b pet-cli-work 2>/dev/null || true
  git add -A && git commit -m 'pet-cli: auto-commit remaining work' || true
fi"""
    return f"""
rc=0
{run}
echo "pet-cli exit code: $rc" >> /logs/agent/pet-cli.txt{commit}
cp {CONFIG_DIR}/logs/llm.log /logs/agent/llm.log 2>/dev/null || true
cp -r {CONFIG_DIR}/sessions /logs/agent/sessions 2>/dev/null || true
exit 0
"""


def launch(
    run_args: list[str], *, model_override: str | None, rebuild: bool, model_hint: str,
    summary: str, view_command: str, extra_args: list[str] | None = None,
    env: dict[str, str] | None = None,
) -> int:
    """CLI 尾巴：解析模型/二进制、拼 --ak/--ae、跑 harness、打印结果位置。"""
    model = resolve_model(model_env(), model_override=model_override, hint=model_hint)
    binary = ensure_linux_binary(rebuild)
    cmd = [*run_args, "-m", str(model["model"]), "--ak", f"binary={binary}"]
    for key in _MODEL_AE_KEYS:
        cmd += ["--ae", f"PET_{key.upper()}={model[key]}"]
    cmd += extra_args or []

    # agent 进程读 env（含 --ae 注入的），容器里的 pet-cli 读 config.yaml
    child_env = dict(os.environ)
    child_env.update({f"PET_{key.upper()}": str(model[key]) for key in _MODEL_AE_KEYS})
    child_env.update(env or {})

    print(f"$ {mask_key(cmd)}\n")
    done = subprocess.run(cmd, env=child_env)
    print(f"\n{summary}\n看轨迹：uv run --project {view_command}")
    return done.returncode