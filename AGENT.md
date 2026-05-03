# AGENT.md

本文件给后续 AI 开发代理使用。目标是让功能迭代保持可运行、可审计、可维护，而不是继续用小 commit 堆出难以收口的复杂度。

## 当前工程判断

这批代码已经形成有价值的产品方向：主动陪伴、长期记忆、管家任务、环境感知和调试面板。但当前状态更接近 alpha：功能密度高，测试意识不错，生产化硬化不足。后续优先做质量收口，再继续扩展新能力。

## 必跑质量门槛

提交前至少运行：

```bash
pnpm exec tsc --noEmit
pnpm build
cd src-tauri && cargo fmt --check
cd src-tauri && cargo test
cd src-tauri && cargo clippy --all-targets -- -D warnings
```

如果某个检查失败，不能只在最终回复里轻描淡写带过。要么修到通过，要么在 TODO 中留下明确的阻塞原因、失败命令和下一步。

## 模块边界

- 不要继续把新逻辑塞进 `src-tauri/src/proactive.rs`。它已经承担 scheduler、gate、prompt rules、reminder、butler schedule、persona/user profile、Tauri command 和测试，后续新增前先拆模块。
- `proactive.rs` 的推荐拆分方向：`proactive/scheduler.rs`、`proactive/gates.rs`、`proactive/prompt.rs`、`proactive/reminders.rs`、`proactive/butler.rs`、`proactive/telemetry.rs`。
- Tauri command 尽量薄：只做参数转换、状态读取和错误映射；业务逻辑放到可测试的纯函数或 service 层。
- 前端大型 panel 不继续膨胀。`PanelDebug.tsx` 和 `PanelMemory.tsx` 的新逻辑优先抽成 hook、子组件或后端聚合 API。
- 避免前后端复制业务规则。像 butler schedule / due 判断这类逻辑应以后端为准，前端消费结构化结果。

## 测试策略

- 解析器、gate、prompt label、排序、过期判断都必须有纯函数单测。
- 修 bug 时补回归测试，尤其是 gate、redaction、tool-call loop、butler schedule 这类隐性行为。
- 后端 label 和前端字典必须有契约测试，防止出现后端已经产出但前端没有描述，或前端保留了幽灵 label。
- 单测覆盖不等于端到端可靠。涉及 Tauri command、LLM tool loop、面板 polling、文件写入和后台任务时，要补 smoke/integration 测试或手动验证记录。

## 安全与工具调用

- LLM 可触发的 `bash`、`write_file`、`edit_file` 属于高风险能力。新增调用路径前必须考虑权限边界、路径范围、命令风险和审计记录。
- tool calling loop 必须有最大轮数、超时和可观测错误；不能让模型无限调用工具。
- 每次工具调用都应带上清晰的目的说明，展示给用户和日志系统，便于判断“为什么要调用这个工具”。
- 高风险工具调用需要 AI 先做结构化风险评估；风险超过阈值时进入人类审核。审核等待必须有超时策略，避免长期无人值守任务被永久卡死。
- 自动化流程不能因为 UI 审核缺席而无限阻塞。默认超时策略应安全、可解释，并写入 decision log / app log。

## 隐私与 Prompt Reinjection

- 任何从本地环境、日历、文件、记忆、历史发言重新注入 prompt 的内容，都必须经过统一 redaction。
- 不要只保护新工具输出；旧的 memory、daily_plan、reminder、mood、persona summary 也可能包含敏感内容。
- redaction 命中率和调用次数要可观测。命中突然变多或长期为零都应方便用户排查配置是否合理。

## 可观测性

- 后台 proactive loop、手动 trigger、panel 按钮触发应尽量复用同一 telemetry 路径，避免调试时看到的数据和真实后台行为不一致。
- 新增 gate、prompt rule、tool review、审核状态时，要同步补 decision log / ToneSnapshot / panel 展示。
- 面板 polling 不应无限增加 IPC 数量。新增多个指标时优先设计聚合 snapshot command，而不是每秒再加一批独立 invoke。

## 前端约束

- 调试面板可以密集，但必须保持可扫描。新增状态优先用紧凑 chip、tooltip、详情折叠，不要继续堆长段文字。
- 对固定格式 UI（stats row、chip strip、toolbar、modal header）要给稳定尺寸、换行和 overflow 策略，避免中文长词或工具名挤爆布局。
- 前端不要承担安全决策的唯一来源。前端可以展示和收集审核结果，最终执行判断应在后端完成。

## Commit 与文档

- 开发时可以小步提交，但合并前按功能主题整理。不要让最终历史只剩大量 `Iter X` 的微提交。
- 每完成一项 TODO，把结果移到 `DONE.md` 并写清日期、行为变化、验证命令。
- 设计变化同步记录到 `IDEA.md`，尤其是 prompt contract、权限模型、tool review 这类会影响后续代理行为的规则。
