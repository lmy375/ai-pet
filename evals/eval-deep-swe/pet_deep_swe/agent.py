"""pet-cli 作为 pier 的 installed agent 跑 DeepSWE：上传静态 Linux 二进制、搭一次性
PET_CONFIG_DIR、``pet-cli -p`` 跑一轮并兜底 commit——verifier 只看已提交的 HEAD。

凭据取 PET_API_BASE / PET_API_KEY / PET_MODEL（宿主 env 或 pier run --ae）；
air-gapped 任务里 LLM 出网走 pier 注入的 HTTPS_PROXY。
"""

from __future__ import annotations

from pier.agents.installed.base import BaseInstalledAgent, with_prompt_template
from pier.agents.network import allowlist_from_urls
from pier.environments.base import BaseEnvironment
from pier.models.agent.context import AgentContext
from pier.models.agent.install import AgentInstallSpec, InstallStep
from pier.models.agent.network import NetworkAllowlist
from pet_eval_common.container import host_extra_ca_cert, pet_cli_env
from pet_eval_common.host import PetCliHostAgent, round_stats, run_script
from pet_eval_common.prompt import render_prompt

from .fixtures import MEMORY_FILES

# DeepSWE 任务允许几十分钟的长活；沙箱内 setup 之类的小命令另说
RUN_TIMEOUT_SEC = 5400

_GIT_IDENTITY = "git config --global user.name pet-cli && git config --global user.email pet-cli@eval.local && git config --global --add safe.directory '*'"


class PetCliAgent(PetCliHostAgent, BaseInstalledAgent):
    """在 DeepSWE 任务容器里运行 pet-cli 单轮对话的 agent。"""

    CONFIG_HINT = "宿主 env 或 pier run --ae；model 也可用 --model 传"

    def __init__(self, *args, binary: str | None = None, **kwargs):
        super().__init__(*args, **kwargs)
        self._binary_override = binary
        self._extra_ca = host_extra_ca_cert()
        self._build_hint = "eval-deep-swe 的 CLI"

    def render_instruction(self, instruction: str) -> str:
        return render_prompt(instruction, require_commit=True)

    @staticmethod
    def name() -> str:
        return "pet-cli"

    def install_spec(self) -> AgentInstallSpec:
        return AgentInstallSpec(
            agent_name=self.name(),
            steps=[
                # 二进制走 setup() 的 upload_file（air-gapped 也可用）；这里只放能进
                # 镜像缓存层的部分。git 身份 root/agent 各配一份——commit 由任务镜像
                # 的 default_user 决定。
                InstallStep(user="root", run=f"mkdir -p /installed-agent && {_GIT_IDENTITY}"),
                InstallStep(user="agent", run=_GIT_IDENTITY),
            ],
        )

    def network_allowlist(self) -> NetworkAllowlist:
        return allowlist_from_urls([self._get_env("PET_API_BASE")])

    async def setup(self, environment: BaseEnvironment) -> None:
        await super().setup(environment)
        await self._install_runtime(environment, MEMORY_FILES)

    @with_prompt_template
    async def run(
        self, instruction: str, environment: BaseEnvironment, context: AgentContext
    ) -> None:
        # one-shot 退出前会等后台任务并续聊；上限略小于整体 RUN_TIMEOUT，
        # 别让默认的 10 分钟提前放弃长任务
        env = self.build_process_env(
            pet_cli_env(
                has_extra_ca=self._extra_ca is not None,
                oneshot_wait_ms=(RUN_TIMEOUT_SEC - 300) * 1000,
            )
        )
        # 兜底 commit 必须在仓库目录（/app）里做；wrapper 的 exit 0 约定见 host.py。
        await self.exec_as_agent(
            environment,
            command=run_script(instruction, auto_commit=True),
            env=env,
            timeout_sec=RUN_TIMEOUT_SEC,
        )

    def populate_context_post_run(self, context: AgentContext) -> None:
        """llm.log 不含 token usage，只能填 n_agent_steps。"""
        rounds, tool_calls = round_stats(self.logs_dir)
        if rounds:
            context.n_agent_steps = rounds
            context.metadata = {"tool_calls": tool_calls}