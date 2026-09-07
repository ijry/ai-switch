# 真实生成测试悬停暂停倒计时实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**目标：** 鼠标移入真实生成测试结果或错误区域时暂停自动关闭倒计时，移出后从剩余时间继续。

**架构：** 保留结果到达时初始化 30 秒倒计时的现有逻辑，将计时器拆为受悬停状态控制的独立 effect。结果与错误容器共享进入、离开处理，关闭或开始下一次测试时清除暂停状态。

**技术栈：** React、TypeScript、Vitest、Testing Library

**规格：** 当前任务用户确认的“从剩余时间继续”交互语义。

## 全局约束

- 不改变手动关闭、新测试进行中隐藏倒计时、结果到达后重置为 30 秒的现有行为。
- 测试必须先失败，再用最小实现修复。
- 仅修改真实生成测试结果区域及其回归测试。

---

### 任务 1：添加悬停暂停回归测试

**文件：**
- 修改：`tests/AccountsScreen.test.tsx`

- [x] 添加测试：倒计时运行后移入结果区域，时间推进时秒数保持不变。
- [x] 添加断言：移出后从被冻结的秒数继续，并最终自动关闭。
- [x] 运行单个测试并确认因缺少暂停行为而失败。

### 任务 2：实现共享悬停暂停逻辑

**文件：**
- 修改：`src/screens/AccountsScreen.tsx`

- [x] 增加真实生成测试自动关闭暂停状态。
- [x] 拆分倒计时初始化与每秒递减 effect，暂停时停止 interval。
- [x] 给结果和错误区域绑定鼠标进入、离开处理。
- [x] 关闭结果或开始新测试时重置暂停状态。

### 任务 3：验证行为与回归

**文件：**
- 验证：`tests/AccountsScreen.test.tsx`
- 验证：`src/screens/AccountsScreen.tsx`

- [x] 运行新增测试并确认通过。
- [x] 运行 `AccountsScreen` 测试文件。
- [x] 运行 TypeScript 类型检查。
- [x] 检查最终差异与工作区状态。
