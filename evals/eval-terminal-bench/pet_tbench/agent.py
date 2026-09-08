"""pet-cli 作为 Harbor 的 installed agent，跑 Terminal-Bench 任务。

Harbor 负责起任务容器、把 instruction.md 交给 agent、跑完后在同一容器里执行
tests/test.sh 并读 /logs/verifier/reward.txt 判分。这里只做三件事：

- 把静态编译的 Linux pet-cli 用 ``upload_file`` 传进容器；
- 在容器里搭一个一次性 PET_CONFIG_DIR（config.yaml + 记忆基线，见 fixtures.py）；
- 用 ``pet-cli -p <instruction>`` 跑一轮，把 llm.log / sessions 导出到 /logs/agent。

模型凭据从 env 取：PET_API_BASE / PET_API_KEY / PET_MODEL（宿主环境或
``harbor run --ae KEY=VALUE``）。

两条 Harbor 的硬规矩决定了 run() 的形状（见 harbor/trial/trial.py）：
- agent 阶段抛任何异常（含 Harbor 自己的超时）→ 直接跳过 verifier，0 分；
  所以 wrapper 永远 exit 0，且要在 Harbor 的超时之前自己结束。
- agent 拿不到任务的 timeout_sec，但 ``environment.environment_dir`` 就是任务的
  environment/ 目录，读它旁边的 task.toml 即可。

接入方式::

    harbor run -d terminal-bench@2.0 -a pet_tbench.agent:PetCliAgent --ak binary=<path>
"""

from __future__ import annotations

import shlex
import subprocess
import tomllib
from pathlib import Path

from harbor.agents.installed.base import BaseInstalledAgent, with_prompt_template
from harbor.environments.base import BaseEnvironment
from harbor.models.agent.context import AgentContext
from pet_eval_common.binary import REPO, resolve_host_binary
from pet_eval_common.container import (
    BINARY,
    CONFIG_DIR,
    EXTRA_CA_PEM,
    ca_setup_cmds,
    config_dir_setup_cmds,
    config_yaml,
    count_llm_rounds,
    host_extra_ca_cert,
    pet_cli_env,
)

from .fixtures import MEMORY_FILES

PROMPT_TEMPLATE = Path(__file__).with_name("prompt.j2")

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


class PetCliAgent(BaseInstalledAgent):
    """在 Terminal-Bench 任务容器里运行 pet-cli 单轮对话的 agent。"""

    def __init__(
        self,
        *args,
        binary: str | None = None,
        timeout_multiplier: float | str = 1.0,
        **kwargs,
    ):
        kwargs.setdefault("prompt_template_path", PROMPT_TEMPLATE)
        # 给了 version Harbor 就不会去容器里探测（pet-cli 也没有 --version）
        kwargs.setdefault("version", _git_short_rev())
        super().__init__(*args, **kwargs)
        self._binary_override = binary
        self._extra_ca = host_extra_ca_cert()
        # 与传给 harbor run --timeout-multiplier 的值保持一致，预算才算得准
        self._timeout_multiplier = float(timeout_multiplier)

    @staticmethod
    def name() -> str:
        return "pet-cli"

    def _config_yaml(self) -> str:
        return config_yaml(
            provider=self._get_env("PET_PROVIDER") or "openai",
            api_base=self._get_env("PET_API_BASE") or "",
            api_key=self._get_env("PET_API_KEY") or "",
            model=self._get_env("PET_MODEL") or self.model_name or "",
            context_window=int(self._get_env("PET_CONTEXT_WINDOW") or 200_000),
            reasoning=self._get_env("PET_REASONING") or "",
            hint="宿主 env 或 harbor run --ae；model 也可用 --model 传",
        )

    async def install(self, environment: BaseEnvironment) -> None:
        binary = resolve_host_binary(self._binary_override, "eval-terminal-bench 的 CLI")
        await environment.upload_file(binary, BINARY)
        cmds = [config_dir_setup_cmds(self._config_yaml(), MEMORY_FILES)]
        if self._extra_ca:
            await environment.upload_file(self._extra_ca, EXTRA_CA_PEM)
            cmds.append(ca_setup_cmds())
        await self.exec_as_root(environment, command="\n".join(cmds))

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
        # one-shot 退出前会等后台任务（spawn_subagent 等）并续聊；上限跟着预算走，
        # 别让默认的 10 分钟提前放弃，也别拖过 timeout
        env = pet_cli_env(
            has_extra_ca=self._extra_ca is not None, oneshot_wait_ms=max(60, budget - 60) * 1000
        )
        # 教训（deep-swe 首跑踩过）：</dev/null 因为 pet-cli 是 TUI 二进制；wrapper 永远
        # exit 0，否则 Harbor 把 trial 当失败处理、跳过 verifier，半成品一分拿不到。
        # 真实退出码记进日志供排查。`timeout` 兜底防 pet-cli 拖过 Harbor 的超时。
        command = f"""
rc=0
echo "budget={budget}s" > /logs/agent/pet-cli.txt
if command -v timeout >/dev/null 2>&1; then
  timeout --foreground {budget}s {BINARY} -p {shlex.quote(instruction)} </dev/null 2>&1 | tee -a /logs/agent/pet-cli.txt || rc=$?
else
  {BINARY} -p {shlex.quote(instruction)} </dev/null 2>&1 | tee -a /logs/agent/pet-cli.txt || rc=$?
fi
echo "pet-cli exit code: $rc" >> /logs/agent/pet-cli.txt
cp {CONFIG_DIR}/logs/llm.log /logs/agent/llm.log 2>/dev/null || true
cp -r {CONFIG_DIR}/sessions /logs/agent/sessions 2>/dev/null || true
exit 0
"""
        await self.exec_as_agent(
            environment, command=command, env=env, timeout_sec=budget + SLACK_SEC // 2
        )

    def populate_context_post_run(self, context: AgentContext) -> None:
        """从导出的 llm.log 里数轮数。llm.log 不含 token usage，只能填 metadata。"""
        rounds, tool_calls = count_llm_rounds(self.logs_dir / "llm.log")
        if rounds:
            context.metadata = {"rounds": rounds, "tool_calls": tool_calls}
