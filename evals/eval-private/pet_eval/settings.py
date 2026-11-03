"""环境变量入口：所有 env 只在这里读一次。"""

from __future__ import annotations

from pathlib import Path

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class EvalSettings(BaseSettings):
    model_config = SettingsConfigDict(extra="ignore")

    # 模型凭据覆盖。都留空时回落到主人真实 config.yaml 里当前 Agent 的配置。
    provider: str = Field("", validation_alias="PET_EVAL_PROVIDER")
    api_base: str = Field("", validation_alias="PET_EVAL_API_BASE")
    api_key: str = Field("", validation_alias="PET_EVAL_API_KEY")
    model: str = Field("", validation_alias="PET_EVAL_MODEL")
    reasoning: str = Field("", validation_alias="PET_EVAL_REASONING")

    # 指定 pet-cli 二进制；留空则找 target/{release,debug}/pet-cli，没有就现编。
    cli_bin: Path | None = Field(None, validation_alias="PET_CLI_BIN")

    # 主人真实的状态根。设了就只读它；不设则按 pet-dev、pet（安装版）的顺序找
    # 第一个配了 key 的当前 Agent。沙箱里每条用例会把它指向自己的临时目录。
    config_dir: Path | None = Field(None, validation_alias="PET_CONFIG_DIR")

    # 只为在非 macOS 上跟 pet-core 的 dirs::config_dir() 保持一致
    xdg_config_home: Path | None = Field(None, validation_alias="XDG_CONFIG_HOME")

    def model_env(self) -> dict[str, str]:
        """模型解析用的 env 覆盖；空串表示没设，回落到 config.yaml。"""
        return {
            "provider": self.provider,
            "api_base": self.api_base,
            "api_key": self.api_key,
            "model": self.model,
            "reasoning": self.reasoning,
        }
