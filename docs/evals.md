# 评测：宠物有没有真把事做完

`evals/eval-private/` 下是一套行为评测。一条用例 = 一句 prompt + workspace 的初始状态 + 做完
之后必须成立的事。跑法是拿真的 `pet-cli -p` 打一次单轮对话——同一个引擎、同一套
系统提示词、同一批工具——只是 `PET_CONFIG_DIR` 指向一个一次性目录。

```bash
uv run --project evals/eval-private pet-eval                    # 全部用例
uv run --project evals/eval-private pet-eval --only edit        # 只跑 id 含 edit 的
uv run --project evals/eval-private pet-eval --repeat 3         # 看方差，按 k/n 汇报
uv run --project evals/eval-private pet-eval --model GPT-5.5    # 同一批用例换个模型
```

模型默认取 `config.yaml` 里当前 Agent 的（评测你实际在用的那只宠物；状态根按 `pet-dev`、
`pet` 的顺序取第一个配了 key 的）。环境变量都在
[settings.py](../evals/eval-private/pet_eval/settings.py) 一处声明（pydantic-settings）：
`PET_EVAL_PROVIDER` / `PET_EVAL_API_BASE` / `PET_EVAL_API_KEY` / `PET_EVAL_MODEL` /
`PET_EVAL_REASONING` 覆盖模型，`PET_CLI_BIN` 指定二进制（不指定就找
`target/{release,debug}/pet-cli`，没有则自动 `cargo build -p pet-cli`），`PET_CONFIG_DIR`
指定唯一的真实配置位置。

## 沙箱

`PET_CONFIG_DIR`（[common.rs](../crates/pet-core/src/common.rs)）能把整个磁盘状态根
换掉，评测就靠它隔离：每条用例一个临时目录，自带 config.yaml、记忆、会话和
`logs/llm.log`，跑完留在 `/tmp/pet-eval/<时间戳>/` 里供事后翻查。
用例可以随便改写 MEMORY.md、删文件，碰不到你真正的宠物。

记忆基线来自 `evals/eval-private/fixtures/memory/`（人设刻意中性、USER/MEMORY 刻意留空），
所以「有没有往 MEMORY.md 里乱塞东西」是可判定的，也不会把你真实的 SOUL.md 拖进来。
沙箱里的 `skills_dir` 指向空目录，`search_api_key` 留空——工具集在每台机器上一致。

轨迹不用额外埋点：`llm.log` 每轮一行 JSON，工具调用和轮数直接从里面还原。

## 写一条用例

`evals/eval-private/cases/<id>.yaml`，路径一律用 `{WORK}` 占位（会替换成沙箱 workspace 绝对路径）：

```yaml
id: tool-edit-uses-edit-file
axis: tool-selection            # 汇总时按这个分组
prompt: "把 {WORK}/server.toml 里的 port 改成 8080，别的不要动。"
seed:                           # 写进 workspace 的初始文件
  server.toml: "port = 3000\n"
setup: ["git init -q"]          # 可选：文件内容表达不了的状态
memory: {}                      # 可选：覆盖 SOUL/USER/MEMORY/HEARTBEAT
expect:
  files:
    server.toml:
      contains: ["port = 8080"]
      not_contains: ["3000"]
      exists: true              # false = 断言它已被删掉
  tools:
    must_use: [edit_file]
    must_not_use: [write_file]
    first_use: read_file        # 第一个动作
    counts: { read_file: 2 }    # 精确次数
    bash_must_not_match: ["sed"]
  memory_written: [HEARTBEAT.md]
  memory_unchanged: [MEMORY.md]
  max_rounds: 4
```

期望里的 key 拼错会直接报错，不会静默跳过——那是评测集烂掉的经典方式。

Stage 1 只有确定性断言（磁盘状态 + 工具轨迹），没有模型裁判：失败是事实不是观点，
红一条就一定意味着行为变了。`counts` 配 `max_rounds` 还能钉住并行——两次调用、
至多两轮，只可能是同一轮里一起发出的。

## 注意

用例会在这台机器上真的执行工具，bash 也在内。prompt 指向沙箱 workspace，但模型
有可能乱走——把一次运行当成在本地跑任何一个 agent 来对待。

## DeepSWE：coding agent 能力

`evals/eval-deep-swe/` 用 [DeepSWE](https://github.com/datacurve-ai/deep-swe)
（113 道真实工程任务，Docker 隔离 + verifier 自动判分，pier 驱动）测 pet-cli。
需要本机 Docker。模型取当前 Agent 的（状态根按 `PET_CONFIG_DIR` > `pet-dev` > `pet` 取第一个
配了 key 的；`PET_PROVIDER` / `PET_API_BASE` / `PET_API_KEY` / `PET_MODEL` 可覆盖）。
首次运行会 clone 任务库并用 clux/muslrust 静态编译 Linux 版 pet-cli（这一步和
容器内 PET_CONFIG_DIR 的搭建在 `evals/pet-eval-common/` 里，Terminal-Bench 共用）。
公司 MITM 代理（Cloudflare Gateway）会拆容器出去的 TLS：设 `PET_EXTRA_CA_CERT`（缺省用
`NODE_EXTRA_CA_CERTS`）指向它的 CA，agent 会把它带进容器——否则打公网 API 一律
"self-signed certificate in certificate chain"（内网网关不受影响）。

```bash
uv run --project evals/eval-deep-swe eval-deep-swe --n-tasks 1   # 冒烟
uv run --project evals/eval-deep-swe eval-deep-swe --only <task-id>
```

结果在 `evals/eval-deep-swe/runs/<job>/`；宠物人设默认不主动 commit，而 verifier
只收已提交的 patch，所以该 agent 的 SOUL/prompt 显式授权 commit 并带兜底提交。

## Terminal-Bench：终端任务能力

`evals/eval-terminal-bench/` 用 [Terminal-Bench 2.0](https://www.tbench.ai)（89 道真实
终端任务，Docker 隔离，跑完在同一容器里执行 `tests/test.sh` 判分，Harbor 驱动）测
pet-cli。需要本机 Docker；模型规则同上。任务库 clone 到 `vendor/` 并钉在 Harbor 注册表
里 `terminal-bench@2.0` 对应的 commit——不经 Harbor Hub（公司 MITM 代理下 Hub 的 TLS
会被拒），`--dataset` 可改走 Hub，`--tasks-path` 可指本地任务目录。

```bash
uv run --project evals/eval-terminal-bench eval-terminal-bench --n-tasks 1        # 冒烟
uv run --project evals/eval-terminal-bench eval-terminal-bench --only build-cython-ext
uv run --project evals/eval-terminal-bench eval-terminal-bench --attempts 3       # pass@3
```

结果在 `evals/eval-terminal-bench/runs/<job>/`（`result.json`；每题 `verifier/reward.txt`、
`agent/pet-cli.txt`、`agent/llm.log`）。Harbor 只要 agent 阶段抛异常（含它自己的超时）
就跳过 verifier 判 0 分，所以 agent 的 wrapper 永远 exit 0，并从任务 `task.toml` 读
`[agent] timeout_sec` 给 pet-cli 一个略小的预算（`timeout` + `PET_ONESHOT_WAIT_MS`）。
这里的 SOUL 不授权 commit——verifier 看的是磁盘最终状态，不是 git。
