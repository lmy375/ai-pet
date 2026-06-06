#!/usr/bin/env bash
# 050 follow-up: 文件行数硬上限检查。rust ≤ 1000、tsx/ts ≤ 600。
# 用 .file-cap-baseline 记录当前已超上限的文件（豁免）；新文件超限直接失败。
#
# 用法：
#   scripts/check-file-cap.sh              # 检查
#   scripts/check-file-cap.sh --baseline   # 重新生成 baseline（landed-fix
#                                            后调用，把降回 cap 以内的文件
#                                            从豁免名单移除）
#
# baseline 文件格式：每行一个 `path<TAB>limit` —— 豁免该 path 直到它降到 limit。
# 简化：先只列豁免 path，全照 cap，超 cap 即写入豁免；用作 "现状容忍 + 不变坏"。

set -euo pipefail
cd "$(dirname "$0")/.."

RUST_CAP=1000
TSX_CAP=600
BASELINE_FILE=".file-cap-baseline"

declare -a OVER=()

while IFS= read -r f; do
  # 跳过 sibling-extracted 测试 body 文件（*_tests.rs / *.test.ts(x)）—— 它们是
  # 050 测试拆出 pattern 的产物，不计入生产 cognitive load。
  case "$f" in
    *_tests.rs|*.test.tsx|*.test.ts) continue ;;
  esac
  lines=$(wc -l < "$f" | tr -d ' ')
  case "$f" in
    src-tauri/src/*.rs) cap=$RUST_CAP ;;
    src/*.tsx|src/*.ts) cap=$TSX_CAP ;;
    *) continue ;;
  esac
  if [ "$lines" -gt "$cap" ]; then
    OVER+=("$f")
  fi
done < <({ find src-tauri/src -type f -name "*.rs" 2>/dev/null;
            find src -type f \( -name "*.tsx" -o -name "*.ts" \) 2>/dev/null; } | sort -u)

if [ "${1:-}" = "--baseline" ]; then
  printf '%s\n' "${OVER[@]}" | sort -u > "$BASELINE_FILE"
  echo "wrote ${#OVER[@]} entries to $BASELINE_FILE"
  exit 0
fi

# 与 baseline 比对：豁免 baseline 内的 path，新增超限直接失败。
# 用 grep -Fxq 避免 bash 3.2 没 declare -A 关联数组。
declare -a NEW_VIOLATORS=()
for f in "${OVER[@]}"; do
  if [ -f "$BASELINE_FILE" ] && grep -Fxq "$f" "$BASELINE_FILE"; then
    continue
  fi
  NEW_VIOLATORS+=("$f")
done

if [ ${#NEW_VIOLATORS[@]} -gt 0 ]; then
  echo "❌ new file-cap violations (not in $BASELINE_FILE):"
  for f in "${NEW_VIOLATORS[@]}"; do
    lines=$(wc -l < "$f" | tr -d ' ')
    case "$f" in
      src-tauri/src/*.rs) cap=$RUST_CAP ;;
      src/*.tsx|src/*.ts) cap=$TSX_CAP ;;
    esac
    echo "  $f: $lines lines (cap $cap)"
  done
  echo ""
  echo "fix: 拆分文件，或 (lifetime decision) run 'scripts/check-file-cap.sh --baseline'"
  exit 1
fi

echo "✓ no new file-cap violations (${#OVER[@]} files in baseline still over cap)"
