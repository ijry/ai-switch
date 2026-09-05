import type { Language } from "../../lib/i18n";

/// Display copy for the Skills AI Switch bundles. The Skill files themselves are
/// English — their frontmatter `description` is a long trigger sentence written for
/// the agent, not for a human reading a list — so the UI renders these instead:
/// a short human name and a one-sentence summary per language.
export type SkillCopy = {
  name: string;
  summary: string;
};

const BUNDLED_SKILL_COPY: Record<string, Record<Language, SkillCopy>> = {
  brainstorming: {
    en: { name: "Brainstorming", summary: "Turns an idea into an approved design and spec before any code gets written." },
    "zh-CN": { name: "头脑风暴", summary: "通过对话把想法整理成规格说明，得到确认后才允许动手写代码。" },
  },
  "dispatching-parallel-agents": {
    en: { name: "Dispatching Parallel Agents", summary: "Splits two or more independent problems across parallel agents, each with its own isolated context." },
    "zh-CN": { name: "并行分派智能体", summary: "把多个互不依赖的问题分派给并行的智能体，各自带独立上下文。" },
  },
  "executing-plans": {
    en: { name: "Executing Plans", summary: "Loads a written plan, reviews it critically, then works through every task in an isolated workspace." },
    "zh-CN": { name: "执行方案", summary: "拿到写好的实施方案后先审阅，再在隔离的工作树里逐项执行。" },
  },
  "finishing-a-development-branch": {
    en: { name: "Finishing a Development Branch", summary: "Runs the full test suite, then presents the merge, PR, or cleanup options for the finished branch." },
    "zh-CN": { name: "收尾分支", summary: "先跑通完整测试，再在合并、开拉取请求和清理工作树之间做选择。" },
  },
  "receiving-code-review": {
    en: { name: "Receiving Code Review", summary: "Verifies each code review comment against the codebase before implementing it, and pushes back if wrong." },
    "zh-CN": { name: "接受评审", summary: "收到代码评审意见后先对着代码核实，该反驳的就反驳，不先照做。" },
  },
  "requesting-code-review": {
    en: { name: "Requesting Code Review", summary: "Dispatches a reviewer subagent over a commit range to catch problems before they spread." },
    "zh-CN": { name: "发起评审", summary: "派一个子智能体评审指定提交区间的改动，在问题扩散前先发现。" },
  },
  "subagent-driven-development": {
    en: { name: "Subagent-Driven Development", summary: "Executes a plan with a fresh subagent per task, a review after each, and one final whole-branch review." },
    "zh-CN": { name: "子智能体开发", summary: "每项任务派一个全新的子智能体实现并评审，最后再整体评审整个分支。" },
  },
  "systematic-debugging": {
    en: { name: "Systematic Debugging", summary: "Requires finding the root cause of a bug or test failure before any fix is proposed." },
    "zh-CN": { name: "系统化调试", summary: "遇到缺陷或测试失败时，必须先定位根因，不允许先猜着改。" },
  },
  "test-driven-development": {
    en: { name: "Test-Driven Development", summary: "Writes the failing test first and watches it fail before any production code is written." },
    "zh-CN": { name: "测试驱动开发", summary: "先写会失败的测试并亲眼看它失败，之后才写实现代码。" },
  },
  "using-git-worktrees": {
    en: { name: "Using Git Worktrees", summary: "Makes sure work happens in an isolated workspace, preferring native tooling over a manual git worktree." },
    "zh-CN": { name: "使用工作树", summary: "确保改动发生在隔离的工作树里，优先用平台自带的工作树工具。" },
  },
  "using-superpowers": {
    en: { name: "Using Superpowers", summary: "Requires checking for a matching Skill and invoking it before any other response or action." },
    "zh-CN": { name: "使用技能", summary: "回应或动手之前，先检查有没有对得上的技能并按它执行。" },
  },
  "verification-before-completion": {
    en: { name: "Verification Before Completion", summary: "Requires running the verification command and reading its output before claiming anything is done." },
    "zh-CN": { name: "完成前验证", summary: "宣称完成之前，必须真的跑一遍验证命令并读完输出。" },
  },
  "writing-plans": {
    en: { name: "Writing Plans", summary: "Turns a spec into a file-by-file implementation plan of bite-sized tasks for someone with no context." },
    "zh-CN": { name: "撰写方案", summary: "把规格说明拆成一份实施方案，任务细到没有背景的人也能照着做。" },
  },
  "writing-skills": {
    en: { name: "Writing Skills", summary: "Builds and edits Skills the TDD way: watch an agent fail without the Skill, then write it." },
    "zh-CN": { name: "编写技能", summary: "用测试驱动开发的方式写技能，先看智能体没有技能时会怎么失败。" },
  },
  "citation-management": {
    en: { name: "Citation Management", summary: "Finds papers, verifies their metadata, and turns DOIs or PMIDs into clean BibTeX entries." },
    "zh-CN": { name: "文献引用管理", summary: "检索论文并核对元数据，把 DOI 或 PMID 转成规范的 BibTeX 条目。" },
  },
  "experimental-design": {
    en: { name: "Experimental Design", summary: "Chooses a study design before data collection: randomization, blocking, and factorial or DOE layouts." },
    "zh-CN": { name: "实验设计", summary: "在采集数据之前定下设计，安排随机化、区组和多因素试验布局。" },
  },
  "exploratory-data-analysis": {
    en: { name: "Exploratory Data Analysis", summary: "Profiles a scientific data file across 200+ formats and reports its structure, quality, and next steps." },
    "zh-CN": { name: "探索性数据分析", summary: "自动识别两百多种科学数据格式，输出结构、质量和后续分析建议。" },
  },
  "hypothesis-generation": {
    en: { name: "Hypothesis Generation", summary: "Turns observations into testable hypotheses with mechanisms, predictions, and experiments to test them." },
    "zh-CN": { name: "假设生成", summary: "把观察结果整理成可检验的假设，配上机制、预测和验证实验。" },
  },
  "paper-lookup": {
    en: { name: "Paper Lookup", summary: "Searches 10 literature APIs such as PubMed, arXiv, and OpenAlex, and reports how each result was found." },
    "zh-CN": { name: "文献检索", summary: "检索 PubMed、arXiv、OpenAlex 等 10 个文献接口，并给出可复现的来源。" },
  },
  "peer-review": {
    en: { name: "Peer Review", summary: "Writes a structured manuscript or grant review covering methods, statistics, and reporting standards." },
    "zh-CN": { name: "同行评审", summary: "按清单写出正式的同行评审意见，覆盖方法、统计和报告规范。" },
  },
  "scholar-evaluation": {
    en: { name: "Scholar Evaluation", summary: "Scores research work with the ScholarEval framework across problem, method, analysis, and writing." },
    "zh-CN": { name: "学术评估", summary: "用 ScholarEval 框架给研究打分，覆盖选题、方法、分析和写作。" },
  },
  "scientific-brainstorming": {
    en: { name: "Scientific Brainstorming", summary: "Explores research directions in open dialogue, looking for gaps, analogies, and cross-field connections." },
    "zh-CN": { name: "科研构想", summary: "在对话中发散研究方向，寻找空白、类比和跨学科的连接。" },
  },
  "scientific-critical-thinking": {
    en: { name: "Scientific Critical Thinking", summary: "Judges evidence quality and spots bias or confounding, using GRADE and Cochrane risk-of-bias frameworks." },
    "zh-CN": { name: "科学批判思维", summary: "评估证据质量，识别偏倚和混杂，用 GRADE 和 Cochrane 偏倚风险框架。" },
  },
  "scientific-schematics": {
    en: { name: "Scientific Schematics", summary: "Generates publication-quality diagrams from a plain-language description, then reviews and refines them." },
    "zh-CN": { name: "科研示意图", summary: "用自然语言描述就能生成可发表水准的示意图，并自动评审和改进。" },
  },
  "scientific-visualization": {
    en: { name: "Scientific Visualization", summary: "Builds journal-ready figures with multi-panel layouts, error bars, and colorblind-safe palettes." },
    "zh-CN": { name: "科研绘图", summary: "做符合期刊要求的插图，含多面板排布、误差棒和色盲友好配色。" },
  },
  "statistical-analysis": {
    en: { name: "Statistical Analysis", summary: "Picks the right test, checks its assumptions, reports effect sizes, and writes results up in APA style." },
    "zh-CN": { name: "统计分析", summary: "选对检验方法，核查前提假定，报告效应量，并按 APA 格式写结果。" },
  },
  "statistical-power": {
    en: { name: "Statistical Power", summary: "Works out the sample size, power, or minimum detectable effect for a study, by formula or simulation." },
    "zh-CN": { name: "统计效力", summary: "用公式或模拟算出所需样本量、统计效力或最小可检测效应。" },
  },
};

export function bundledSkillCopy(skillId: string, language: Language): SkillCopy | undefined {
  return BUNDLED_SKILL_COPY[skillId]?.[language];
}

export function bundledSkillIds(): string[] {
  return Object.keys(BUNDLED_SKILL_COPY);
}
