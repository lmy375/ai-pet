"""Docker 类评测（eval-deep-swe、eval-terminal-bench）共用的宿主侧工具。

两个 harness 各自是独立的 uv 项目（pier vs Harbor 依赖不同），但宿主侧骨架
（host.py）、把 pet-cli 送进容器、搭一次性 PET_CONFIG_DIR、从 llm.log 数轮数、
挑当前 Agent 的模型这几件事完全一样，放这里只写一份。
"""
