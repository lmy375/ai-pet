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

import json
import shlex
import uuid
from pathlib import Path

import yaml
from pier.agents.installed.base import BaseInstalledAgent, with_prompt_template
from pier.agents.network import allowlist_from_urls
from pier.environments.base import BaseEnvironment
from pier.models.agent.context import AgentContext
from pier.models.agent.install import AgentInstallSpec, InstallStep
from pier.models.agent.network import NetworkAllowlist

from .fixtures import MEMORY_FILES

REPO = Path(__file__).resolve().parents[3]
DEFAULT_BINARY = REPO / "target/musl/x86_64-unknown-linux-musl/release/pet-cli"
PROMPT_TEMPLATE = Path(__file__).with_name("prompt.j2")

# 容器内路径
BINARY = "/installed-agent/pet-cli"
CONFIG_DIR = "/pet-config"
AGENT_ID = "eval"

# DeepSWE 任务允许几十分钟的长活；沙箱内 setup 之类的小命令另说
RUN_TIMEOUT_SEC = 5400

_GIT_IDENTITY = (
    "git config --global user.name pet-cli && "
    "git config --global user.email pet-cli@eval.local && "
    "git config --global --add safe.directory '*'"
)


def _write_file_cmd(path: str, content: str) -> str:
    """生成把 content 原样写进容器文件的 shell 片段（quoted heredoc，不做展开）。"""
    marker = f"PET_EOF_{uuid.uuid4().hex[:8]}"
    return f"cat > {shlex.quote(path)} <<'{marker}'\n{content}\n{marker}"


class PetCliAgent(BaseInstalledAgent):
    """在 DeepSWE 任务容器里运行 pet-cli 单轮对话的 agent。"""

    def __init__(self, *args, binary: str | None = None, **kwargs):
        kwargs.setdefault("prompt_template_path", PROMPT_TEMPLATE)
        super().__init__(*args, **kwargs)
        self._binary_override = binary

    @staticmethod
    def name() -> str:
        return "pet-cli"

    def _host_binary(self) -> Path:
        """宿主机上的 Linux pet-cli；--ak binary=… 与 PET_CLI_LINUX_BIN 可覆盖。"""
        raw = self._binary_override or self._get_env("PET_CLI_LINUX_BIN")
        binary = Path(raw) if raw else DEFAULT_BINARY
        if not binary.exists():
            raise FileNotFoundError(
                f"找不到 Linux 版 pet-cli：{binary}\n"
                "先用 eval-deep-swe 的 CLI 构建（docker + clux/muslrust），"
                "或设 PET_CLI_LINUX_BIN 指向现成的二进制。"
            )
        return binary

    def _config_yaml(self) -> str:
        api_base = self._get_env("PET_API_BASE") or ""
        api_key = self._get_env("PET_API_KEY") or ""
        model = self._get_env("PET_MODEL") or self.model_name or ""
        if not api_base or not model:
            raise ValueError(
                "需要模型配置：设 PET_API_BASE / PET_API_KEY / PET_MODEL"
                "（宿主 env 或 pier run --ae），model 也可用 --model 传"
            )
        # 与 eval-private sandbox._config_yaml 同款：AppSettings 其余字段有 serde default
        return yaml.safe_dump(
            {
                "skills_dir": f"{CONFIG_DIR}/skills",
                "search_api_key": "",  # 无 Tavily key ⇒ 无 web_search，工具集固定
                "active_agent": AGENT_ID,
                "agents": [
                    {
                        "id": AGENT_ID,
                        "name": "小宠",
                        "api_base": api_base,
                        "api_key": api_key,
                        "model": model,
                        "context_window": int(self._get_env("PET_CONTEXT_WINDOW") or 200_000),
                        "reasoning_effort": self._get_env("PET_REASONING_EFFORT") or "",
                    }
                ],
            },
            allow_unicode=True,
            sort_keys=False,
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

        await environment.upload_file(self._host_binary(), BINARY)

        parts = [
            f"chmod 755 {BINARY}",
            f"mkdir -p {CONFIG_DIR}/memory/{AGENT_ID} {CONFIG_DIR}/skills"
            f" {CONFIG_DIR}/sessions {CONFIG_DIR}/logs",
            _write_file_cmd(f"{CONFIG_DIR}/config.yaml", self._config_yaml()),
        ]
        for filename, content in MEMORY_FILES.items():
            parts.append(_write_file_cmd(f"{CONFIG_DIR}/memory/{AGENT_ID}/{filename}", content))
        # 容器是单任务一次性的，宽松权限即可让任意 default_user 读写 sessions/logs
        parts.append(f"chmod -R a+rwX {CONFIG_DIR}")
        await self.exec_as_root(environment, command="\n".join(parts))

    @with_prompt_template
    async def run(
        self, instruction: str, environment: BaseEnvironment, context: AgentContext
    ) -> None:
        env = self.build_process_env(
            {
                "PET_CONFIG_DIR": CONFIG_DIR,
                # one-shot 退出前会等后台任务（spawn_subagent 等）并续聊；上限给到
                # 略小于整体 RUN_TIMEOUT，别让默认的 10 分钟提前放弃长任务
                "PET_ONESHOT_WAIT_MS": str((RUN_TIMEOUT_SEC - 300) * 1000),
            }
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
        """从导出的 llm.log 里数轮数（格式同 eval-private 的 trace.py）。

        llm.log 每轮一行 JSON，但不含 token usage，所以只能填 n_agent_steps。
        纯 best-effort：解析失败不影响判分。
        """
        log = self.logs_dir / "llm.log"
        try:
            lines = log.read_text(encoding="utf-8").splitlines()
        except OSError:
            return
        rounds = 0
        tool_calls = 0
        for line in lines:
            try:
                entry = json.loads(line)
            except json.JSONDecodeError:
                continue
            if ":sub:" not in str(entry.get("session_id", "")):
                rounds += 1
            tool_calls += len((entry.get("response") or {}).get("tool_calls") or [])
        if rounds:
            context.n_agent_steps = rounds
            context.metadata = {"tool_calls": tool_calls}
