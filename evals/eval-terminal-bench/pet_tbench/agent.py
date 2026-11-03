"""pet-cli 作为 Harbor 的 installed agent 跑 Terminal-Bench：上传二进制、搭一次性
PET_CONFIG_DIR、``pet-cli -p`` 跑一轮并导出日志。凭据来自 PET_API_BASE / PET_API_KEY /
PET_MODEL（宿主 env 或 harbor run --ae）。

Harbor 规矩：agent 阶段抛异常（含超时）会跳过 verifier 判 0 分，所以 wrapper 永远
exit 0 且要自己先结束；``environment.environment_dir`` 旁可读 task.toml 的 timeout_sec。
"""

from __future__ import annotations

import subprocess
import tomllib
from pathlib import Path

from harbor.agents.installed.base import BaseInstalledAgent, with_prompt_template
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext
from pet_eval_common.binary import REPO
from pet_eval_common.container import host_extra_ca_cert, pet_cli_env
from pet_eval_common.host import PetCliHostAgent, round_stats, run_script
from pet_eval_common.prompt import render_prompt

from .fixtures import MEMORY_FILES

# 任务没写 [agent] timeout_sec 时 Harbor 不设外层超时；我们仍给 pet-cli 一个上限
DEFAULT_TIMEOUT_SEC = 1800
# 在 Harbor 的超时之前留出的余量：导日志 + Harbor 自己的收尾
SLACK_SEC = 120
MIN_BUDGET_SEC = 300


def _git_short_rev() -> str:
    try:
        return subprocess.run(
            ["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip() or "dev"
    except (OSError, subprocess.CalledProcessError):
        return "dev"


class PetCliAgent(PetCliHostAgent, BaseInstalledAgent):
    """在 Terminal-Bench 任务容器里运行 pet-cli 单轮对话的 agent。"""

    CONFIG_HINT = "宿主 env 或 harbor run --ae；model 也可用 --model 传"

    def __init__(
        self, *args, binary: str | None = None,
        timeout_multiplier: float | str = 1.0, **kwargs,
    ):
        # 给了 version Harbor 就不会去容器里探测（pet-cli 也没有 --version）
        kwargs.setdefault("version", _git_short_rev())
        super().__init__(*args, **kwargs)
        self._binary_override = binary
        self._extra_ca = host_extra_ca_cert()
        self._build_hint = "eval-terminal-bench 的 CLI"
        # 与 harbor run --timeout-multiplier 保持一致，预算才算得准
        self._timeout_multiplier = float(timeout_multiplier)

    def render_instruction(self, instruction: str) -> str:
        return render_prompt(instruction, require_commit=False)

    @staticmethod
    def name() -> str:
        return "pet-cli"

    async def install(self, environment: BaseEnvironment) -> None:
        await self._install_runtime(environment, MEMORY_FILES)

    def _budget_sec(self, environment: BaseEnvironment) -> int:
        """pet-cli 本轮可用的秒数：任务 [agent].timeout_sec × multiplier − 余量。"""
        timeout = DEFAULT_TIMEOUT_SEC
        try:
            task_toml = Path(environment.environment_dir).parent / "task.toml"
            raw = tomllib.loads(task_toml.read_text(encoding="utf-8"))
            timeout = float((raw.get("agent") or {}).get("timeout_sec") or timeout)
        except (OSError, AttributeError, tomllib.TOMLDecodeError, ValueError):
            pass
        return max(MIN_BUDGET_SEC, int(timeout * self._timeout_multiplier) - SLACK_SEC)

    @with_prompt_template
    async def run(
        self, instruction: str, environment: BaseEnvironment, context: AgentContext
    ) -> None:
        budget = self._budget_sec(environment)
        # one-shot 退出前会等后台任务并续聊；上限跟着预算走，别拖过 Harbor 的 timeout
        env = pet_cli_env(
            has_extra_ca=self._extra_ca is not None, oneshot_wait_ms=max(60, budget - 60) * 1000
        )
        await self.exec_as_agent(
            environment,
            command=run_script(instruction, budget_sec=budget),
            env=env,
            timeout_sec=budget + SLACK_SEC // 2,
        )

    def populate_context_post_run(self, context: AgentContext) -> None:
        """llm.log 不含 token usage，只能填 metadata。"""
        rounds, tool_calls = round_stats(self.logs_dir)
        if rounds:
            context.metadata = {"rounds": rounds, "tool_calls": tool_calls}