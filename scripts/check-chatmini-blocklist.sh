#!/usr/bin/env bash
# 064-part3: ChatMini 主视图禁渲染 audit / 游戏化 / 决策推理。本脚本 grep
# 一组明确的 "不该出现" 标记，命中即失败。
#
# 范围：src/components/ChatMini*.tsx 文件。
# 例外：mood / motion / pet 表达 emoji 由 LLM 生成（出现在 inline string
# 字面量内但非 JSX 属性），无法仅靠 grep 区分；该层防御靠 064 part-2 视觉
# review。本 lint 只挡明显违规：装饰 emoji 当 chip icon、audit chip 名、
# 游戏化 paws/diamonds 计数等。
#
# 用法：scripts/check-chatmini-blocklist.sh

set -euo pipefail
cd "$(dirname "$0")/.."

# 黑名单 token：找到任一即红。每条都对应 064 spec 内明令删除的类别。
# 模糊 token（🐾 / ✦ 单字）禁要求带计数后缀，避免误伤用户 assistant glyph
# 默认 fallback。
PATTERNS=(
  '🐾[ 　]*[0-9]'      # paws 计数 (gamification)
  '✦[ 　]*[0-9]'       # diamonds 计数 (gamification)
  '累计.*天.*陪伴'     # 累计 N 天陪伴 chip
  '本周.*[0-9]+.*次'   # 本周 N 次 chip
  '今日.*[0-9]+.*次'   # 今日 N 次 chip
  'Asia/Shanghai'      # 时区 chip
  '<Silent>'           # 决策推理 marker (与 064-part1 互补)
  'decision[_-]recent' # 决策最近态 chip
)

# ChatMini 主视图文件白名单。子组件除外（panelChatBits 等不属于 ChatMini 主
# 视图边界）。
FILES=()
while IFS= read -r f; do
  FILES+=("$f")
done < <(find src/components -maxdepth 2 -type f -name "ChatMini*.tsx" 2>/dev/null)

if [ ${#FILES[@]} -eq 0 ]; then
  echo "✓ no ChatMini*.tsx files found — vacuously clean"
  exit 0
fi

HITS=0
for f in "${FILES[@]}"; do
  for p in "${PATTERNS[@]}"; do
    # -F 不需要正则；但部分 pattern 含 . 元字符，仍想 regex 匹配 — 用 -E。
    # 严格匹配 quote / JSX 文本两种形式，全文 grep 报行号让 dev 一眼定位。
    if grep -nE "$p" "$f" >/dev/null 2>&1; then
      echo "❌ $f hits blocklist token: $p"
      grep -nE "$p" "$f" | head -3 | sed 's/^/    /'
      HITS=$((HITS + 1))
    fi
  done
done

if [ "$HITS" -gt 0 ]; then
  echo ""
  echo "$HITS blocklist hit(s) in ChatMini 主视图 — 与 064 / feedback_pet_core_5"
  echo "（audit / 游戏化 / 决策推理不上主视图）冲突。删元素或移到 PanelDebug。"
  exit 1
fi

echo "✓ no ChatMini blocklist hits in ${#FILES[@]} file(s)"
