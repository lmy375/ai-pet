# PanelMemory 类目折叠状态持久化

## 需求

PanelMemory 每个 category（butler_tasks / todo / ai_insights / general 等）
> 10 条时折叠到前 5，点"展开全部 N 条"按钮可切到展开。但状态只 session
内有效——关 panel / 重启 pet 再回来，每次都要再点。让用户"我总要展开这
几类"的稳定偏好跨重启保留。

## 实现

`src/components/panel/PanelMemory.tsx`：

- `expandedCategories: useState<Set<string>>` 的 initializer 改为 lazy：
  - 读 `localStorage["pet-memory-expanded-cats"]`
  - JSON parse → 校验 array of string → Set
  - 失败 / 缺失 → empty Set（与原默认一致）
- 既有 toggle 按钮 onClick 里同步写回 localStorage：JSON.stringify([...next])
- try/catch 包写入：私密浏览 / 配额满静默退化，本次 UI 仍生效，下次启动
  退回 empty Set

## 与 pinnedKeys 一致

storage key 命名延续 `pet-memory-*` 前缀；序列化与 `pinnedKeys` 相同的
"Array.from(set) → JSON" 路径，方便后续 stat-utils 统一管理。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 入 PanelMemory，点 todo 类目"展开全部"→ 显全集
  - 关 panel → 再开 → todo 仍展开（不必再点）
  - 在 butler_tasks 也点展开 → 关 panel 重开 → 两个类目都展开
  - 再次点击折叠 → 关重开 → 折叠状态也持久
  - localStorage 不可用（私密窗口）→ UI 仍工作，session 内有效；console
    无报错

## 不在本轮范围

- 没改"自动折叠阈值"（> 10 条）：那是产品默认偏好，单独配置成本大
- 没做"全部展开 / 全部折叠"快捷：单类目控制颗粒已合适；批量切换可作单独
  需求

## TODO 池剩余

- ChatMini ⌘L 聚焦输入框
- PanelTasks ⌘N 全屏 quick-add 模态
- PanelDebug stats 一键导出 markdown
