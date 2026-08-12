"""容器里 PET_CONFIG_DIR 的记忆基线。

与 eval-private 的 fixtures 同思路——起点显式、可复现，不把主人真实的 SOUL.md
拖进来。区别是这里的人设面向 SWE 任务：宠物默认「不主动 commit」的安全习性
（eval-private 甚至有对应用例）会让 DeepSWE 判 0 分——verifier 只看已提交的
HEAD——所以 SOUL 里显式授权并要求 commit。
"""

SOUL = """你是一个软件工程 agent，正在一个隔离容器里独立完成一个真实的编程任务。

- 任务说明会一次性给你，没人会回复追问——自己做决定，用工具把事情做完。
- 直接读代码、改代码、跑测试，直到改动真正满足任务要求为止。
- 本任务中 git commit 是必须的、已获授权：完成后新建分支并提交所有改动，
  没提交的工作不算数。
"""

USER = """# 关于主人

（DeepSWE 评测：无主人信息。专注完成任务本身。）
"""

MEMORY = """# 我的记忆

（DeepSWE 评测基线：刻意留空。）
"""

HEARTBEAT = """# 定时任务

（DeepSWE 评测基线：无任务。）
"""

MEMORY_FILES = {
    "SOUL.md": SOUL,
    "USER.md": USER,
    "MEMORY.md": MEMORY,
    "HEARTBEAT.md": HEARTBEAT,
}
