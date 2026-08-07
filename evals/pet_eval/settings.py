"""环境变量入口。

所有 env 只在这里读一次，别的模块拿 `EvalSettings` 实例，不再各自 `os.environ.get`。
"""

from __future__ import annotations

import sys
from pathlib import Path

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class EvalSettings(BaseSettings):
    model_config = SettingsConfigDict(extra="ignore")

    # 模型凭据覆盖。都留空时回落到主人真实 config.yaml 里当前 Agent 的配置。
    api_base: str = Field("", validation_alias="PET_EVAL_API_BASE")
    api_key: str = Field("", validation_alias="PET_EVAL_API_KEY")
    model: str = Field("", validation_alias="PET_EVAL_MODEL")

    # 指定 pet-cli 二进制；留空则找 target/{release,debug}/pet-cli，没有就现编。
    cli_bin: Path | None = Field(None, validation_alias="PET_CLI_BIN")

    # 主人真实的状态根。设了就用它读 config.yaml——评测默认跑他实际在用的那个模型。
    # （沙箱是另一回事：每条用例把这个变量指向自己的临时目录后再起 pet-cli。）
    config_dir: Path | None = Field(None, validation_alias="PET_CONFIG_DIR")

    # 只为在非 macOS 上跟 pet-core 的 dirs::config_dir() 保持一致
    xdg_config_home: Path | None = Field(None, validation_alias="XDG_CONFIG_HOME")

    def real_config_dir(self) -> Path:
        if self.config_dir is not None:
            return self.config_dir
        if sys.platform == "darwin":
            return Path.home() / "Library/Application Support/pet"
        return (self.xdg_config_home or Path.home() / ".config") / "pet"
