# PanelTasks priority badge 右键打开 picker

## 需求

priority badge 左键已可开 inline picker。右键当前会触发任务卡的 `taskCtxMenu`
（行级菜单），与 priority 调整不直接相关。右键直接打 priority picker
对鼠标用户更顺手 —— 不必精准对左键。

## 实现

`src/components/panel/PanelTasks.tsx` priority badge button 加 `onContextMenu`：

- preventDefault 吃浏览器默认 ctx menu
- stopPropagation 抢在任务卡级 onContextMenu（taskCtxMenu）之前，防
  两个菜单同时弹
- toggle 行为与 onClick 完全一致：`setPriorityPickerTitle(cur ===
  t.title ? null : t.title)`
- title tooltip 文案补"点击 / 右键"双入口提示

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 左键 priority badge → picker 开
  - 右键 priority badge → 同款 picker 开（不触发任务卡 ctx menu）
  - 右键任务卡其它区域 → 仍弹任务卡 ctx menu（既有行为不变）
  - picker 已开时再右键 → 同 toggle 关
  - 浏览器默认右键菜单被吃，不闪现

## 不在本轮范围

- 没改 priority chip filter 行的右键行为：那是 filter chip，不是改
  task 优先级
- 没让右键支持"快速 +1 / -1"（一键调整不开 picker）：picker 已经显
  完整 P0..9 选项，覆盖快慢用户；微调能加但不必
- 没让 status badge 同款（与右键改 priority 对称）：status 切换是 done
  / cancelled 等不可逆动作；保留左键 + ctx menu 二次确认路径更稳

## TODO 池剩余

空。下一轮需自主提需求。
