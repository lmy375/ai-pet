"""容器内那一套：pet-cli 放哪、PET_CONFIG_DIR 怎么搭、llm.log 怎么数轮数。"""

from __future__ import annotations

import json
import os
import shlex
import uuid
from pathlib import Path

import yaml

# 容器内路径（两个 harness 一致）
BINARY = "/installed-agent/pet-cli"
EXTRA_CA_PEM = "/installed-agent/extra-ca.pem"
CONFIG_DIR = "/pet-config"
CA_BUNDLE = f"{CONFIG_DIR}/ca-bundle.pem"
AGENT_ID = "eval"

# 常见发行版的系统 CA bundle（Debian/Ubuntu/Alpine → 前者，RHEL 系 → 后者）
_SYSTEM_CA_BUNDLES = ("/etc/ssl/certs/ca-certificates.crt", "/etc/pki/tls/certs/ca-bundle.crt")


def host_extra_ca_cert() -> Path | None:
    """宿主要带进容器的额外 CA（PEM）：PET_EXTRA_CA_CERT，或 Node 那套 NODE_EXTRA_CA_CERTS。

    公司 Cloudflare Gateway 这类 MITM 代理会拆容器出去的 TLS；任务镜像不认它的 CA，
    pet-cli（openssl + rustls-native-certs）和任务里的 pip/curl 就全挂在 "self-signed
    certificate in certificate chain" 上。内网网关不被拆，所以走 litellm 时看不到这问题。
    """
    raw = os.environ.get("PET_EXTRA_CA_CERT") or os.environ.get("NODE_EXTRA_CA_CERTS")
    if not raw:
        return None
    path = Path(raw)
    if not path.is_file():
        raise FileNotFoundError(f"额外 CA 文件不存在：{path}")
    return path


def ca_setup_cmds() -> str:
    """以 root 执行：把 EXTRA_CA_PEM 追加进系统 bundle（任务里的 pip/curl 用），并拼一份
    CA_BUNDLE 给 pet-cli 走 SSL_CERT_FILE（镜像没有系统 bundle 时也能用）。"""
    lines = [f"cp {EXTRA_CA_PEM} {CA_BUNDLE}"]
    for bundle in _SYSTEM_CA_BUNDLES:
        lines.append(
            f"if [ -f {bundle} ]; then cat {bundle} {EXTRA_CA_PEM} > {CA_BUNDLE};"
            f" cat {EXTRA_CA_PEM} >> {bundle}; fi"
        )
    return "\n".join(lines)


def pet_cli_env(*, has_extra_ca: bool, oneshot_wait_ms: int) -> dict[str, str]:
    """跑 pet-cli 的进程 env。"""
    env = {"PET_CONFIG_DIR": CONFIG_DIR, "PET_ONESHOT_WAIT_MS": str(oneshot_wait_ms)}
    if has_extra_ca:
        env["SSL_CERT_FILE"] = CA_BUNDLE
    return env


def write_file_cmd(path: str, content: str) -> str:
    """生成把 content 原样写进容器文件的 shell 片段（quoted heredoc，不做展开）。"""
    marker = f"PET_EOF_{uuid.uuid4().hex[:8]}"
    return f"cat > {shlex.quote(path)} <<'{marker}'\n{content}\n{marker}"


def config_yaml(
    *,
    provider: str = "openai",
    api_base: str,
    api_key: str,
    model: str,
    context_window: int = 200_000,
    reasoning: str = "",
    hint: str,
) -> str:
    """一次性 config.yaml。字段名对齐 pet-core settings.rs 的 AgentConfig；其余字段有 serde default。"""
    if not api_base or not model:
        raise ValueError(f"需要模型配置：设 PET_API_BASE / PET_API_KEY / PET_MODEL（{hint}）")
    return yaml.safe_dump(
        {
            "skills_dir": f"{CONFIG_DIR}/skills",
            "search_api_key": "",  # 无 Tavily key ⇒ 无 web_search，工具集固定
            "active_agent": AGENT_ID,
            "agents": [
                {
                    "id": AGENT_ID,
                    "name": "小宠",
                    "provider": provider,
                    "api_base": api_base,
                    "api_key": api_key,
                    "model": model,
                    "context_window": context_window,
                    "reasoning": reasoning,
                }
            ],
        },
        allow_unicode=True,
        sort_keys=False,
    )


def config_dir_setup_cmds(config: str, memory_files: dict[str, str]) -> str:
    """以 root 执行的一段 shell：chmod 二进制、建 PET_CONFIG_DIR、写 config.yaml 与记忆基线。"""
    parts = [
        f"chmod 755 {BINARY}",
        f"mkdir -p {CONFIG_DIR}/memory/{AGENT_ID} {CONFIG_DIR}/skills"
        f" {CONFIG_DIR}/sessions {CONFIG_DIR}/logs",
        write_file_cmd(f"{CONFIG_DIR}/config.yaml", config),
    ]
    for filename, content in memory_files.items():
        parts.append(write_file_cmd(f"{CONFIG_DIR}/memory/{AGENT_ID}/{filename}", content))
    # 容器是单任务一次性的，宽松权限即可让任意 default_user 读写 sessions/logs
    parts.append(f"chmod -R a+rwX {CONFIG_DIR}")
    return "\n".join(parts)


def count_llm_rounds(log: Path) -> tuple[int, int]:
    """从导出的 llm.log 数 (主会话轮数, 工具调用数)。格式同 eval-private 的 trace.py。

    每轮一行 JSON，不含 token usage。读不到 / 解析失败一律算 0——纯 best-effort。
    """
    try:
        lines = log.read_text(encoding="utf-8").splitlines()
    except OSError:
        return 0, 0
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
    return rounds, tool_calls
