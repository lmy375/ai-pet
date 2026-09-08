"""pet-cli 作为 pier（Harbor fork）的 installed agent，跑 DeepSWE 任务。

pier 负责起任务容器（air-gapped）、把 instruction.md 交给 agent、跑完后由
verifier 从 git 提交里收 patch 并判分。这里只做三件事：

- 把静态编译的 Linux pet-cli 用 ``upload_file`` 传进容器（不需要容器有网）；
- 在容器里搭一个一次性 PET_CONFIG_DIR（config.yaml + 记忆基线，
  与 eval-private 的沙箱同思路，见 fixtures.py）；
- 用 ``pet-cli -p <instruction>`` 跑一轮，结束后兜底 commit——verifier 只看
  已提交的 HEAD。

模型凭据从 env 取：PET_API_BASE / PET_API_KEY / PET_MODEL（宿主环境或
``pier run --ae KEY=VALUE``）。air-gapped 任务里 LLM 出网走 pier 注入的
HTTPS_PROXY，pet-core 的 reqwest 默认就吃这个变量。

接入方式::

    pier run -p <tasks> --agent-import-path pet_deep_swe.agent:PetCliAgent
"""

from __future__ import annotations

import shlex
from pathlib import Path

from pier.agents.installed.base import BaseInstalledAgent, with_prompt_template
from pier.agents.network import allowlist_from_urls
from pier.environments.base import BaseEnvironment
from pier.models.agent.context import AgentContext
from pier.models.agent.install import AgentInstallSpec, InstallStep
from pier.models.agent.network import NetworkAllowlist
from pet_eval_common.binary import resolve_host_binary
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

# DeepSWE 任务允许几十分钟的长活；沙箱内 setup 之类的小命令另说
RUN_TIMEOUT_SEC = 5400

_GIT_IDENTITY = (
    "git config --global user.name pet-cli && "
    "git config --global user.email pet-cli@eval.local && "
    "git config --global --add safe.directory '*'"
)


class PetCliAgent(BaseInstalledAgent):
    """在 DeepSWE 任务容器里运行 pet-cli 单轮对话的 agent。"""

    def __init__(self, *args, binary: str | None = None, **kwargs):
        kwargs.setdefault("prompt_template_path", PROMPT_TEMPLATE)
        super().__init__(*args, **kwargs)
        self._binary_override = binary
        self._extra_ca = host_extra_ca_cert()

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
            hint="宿主 env 或 pier run --ae；model 也可用 --model 传",
        )

    def install_spec(self) -> AgentInstallSpec:
        return AgentInstallSpec(
            agent_name=self.name(),
            steps=[
                # 二进制本体走 setup() 里的 upload_file（air-gapped 也可用）；
                # 这里只放可以进镜像缓存层的部分。git 身份 root 和 agent 用户各配
                # 一份——commit 由哪个用户执行取决于任务镜像的 default_user。
                InstallStep(user="root", run=f"mkdir -p /installed-agent && {_GIT_IDENTITY}"),
                InstallStep(user="agent", run=_GIT_IDENTITY),
            ],
        )

    def network_allowlist(self) -> NetworkAllowlist:
        return allowlist_from_urls([self._get_env("PET_API_BASE")])

    async def setup(self, environment: BaseEnvironment) -> None:
        await super().setup(environment)

        binary = resolve_host_binary(self._binary_override, "eval-deep-swe 的 CLI")
        await environment.upload_file(binary, BINARY)
        cmds = [config_dir_setup_cmds(self._config_yaml(), MEMORY_FILES)]
        if self._extra_ca:
            await environment.upload_file(self._extra_ca, EXTRA_CA_PEM)
            cmds.append(ca_setup_cmds())
        await self.exec_as_root(environment, command="\n".join(cmds))

    @with_prompt_template
    async def run(
        self, instruction: str, environment: BaseEnvironment, context: AgentContext
    ) -> None:
        # one-shot 退出前会等后台任务（spawn_subagent 等）并续聊；上限给到
        # 略小于整体 RUN_TIMEOUT，别让默认的 10 分钟提前放弃长任务
        env = self.build_process_env(
            pet_cli_env(
                has_extra_ca=self._extra_ca is not None,
                oneshot_wait_ms=(RUN_TIMEOUT_SEC - 300) * 1000,
            )
        )
        # pet-cli 失败也要兜底 commit、导出日志。注意两点教训（首跑踩过）：
        # - 兜底 commit 必须在仓库目录（/app）里做，exec 的默认 cwd 不是它；
        # - wrapper 永远 exit 0：把 pet-cli 的退出码传给 pier 会让它把 trial 当
        #   失败处理、跳过 verifier.collect，半成品工作一分拿不到。真实退出码
        #   记进日志供排查。
        command = f"""
rc=0
{BINARY} -p {shlex.quote(instruction)} </dev/null 2>&1 | tee /logs/agent/pet-cli.txt || rc=$?
echo "pet-cli exit code: $rc" >> /logs/agent/pet-cli.txt
cd /app
if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
  git checkout -b pet-cli-work 2>/dev/null || true
  git add -A && git commit -m 'pet-cli: auto-commit remaining work' || true
fi
cp {CONFIG_DIR}/logs/llm.log /logs/agent/llm.log 2>/dev/null || true
cp -r {CONFIG_DIR}/sessions /logs/agent/sessions 2>/dev/null || true
exit 0
"""
        await self.exec_as_agent(
            environment, command=command, env=env, timeout_sec=RUN_TIMEOUT_SEC
        )

    def populate_context_post_run(self, context: AgentContext) -> None:
        """从导出的 llm.log 里数轮数。llm.log 不含 token usage，所以只能填 n_agent_steps。"""
        rounds, tool_calls = count_llm_rounds(self.logs_dir / "llm.log")
        if rounds:
            context.n_agent_steps = rounds
            context.metadata = {"tool_calls": tool_calls}
