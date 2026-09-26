# Glossary — English / 中文对照表

Owner: assistant. English is canonical: code, issues, commits, ADRs and freezes use the English
column, and this table exists so the architect can read and discuss those artifacts without
re-deriving the terms. The Chinese column is a working rendering, not an official RISC-V
translation; if you prefer another word, change only that cell.

## 1. 处理器与指令 / Processor and instructions

| English | 中文 | Note in this project |
|---|---|---|
| instruction | 指令 | One 32-bit or 16-bit word executed by the hart |
| encoding | 编码 | The raw instruction word, printed as `0x<8 hex digits>` |
| opcode | 操作码 | Low 7 bits that select the instruction class |
| funct3 / funct7 | 功能字段 | Sub-selectors inside an opcode |
| immediate | 立即数 | Constant carried inside the instruction |
| register | 寄存器 | `x0`–`x31` |
| general-purpose register (GPR) | 通用寄存器 | Excludes CSRs and F registers |
| program counter (PC) | 程序计数器 | Address of the instruction being retired |
| instruction fetch | 取指 | Reading the instruction word at PC |
| load | 加载 | Memory → register |
| store | 存储 | Register → memory |
| sign extension | 符号扩展 | What `lb`/`lh`/`lw` do |
| zero extension | 零扩展 | What `lbu`/`lhu`/`lwu` do |
| misaligned access | 非对齐访问 | Legal for data (Zicclsm), traps for fetch |
| branch | 分支 | Conditional PC change |
| jump | 跳转 | `jal`, `jalr` |
| compressed instruction | 压缩指令 | 16-bit encodings; slice S5 |
| shift | 移位 | `sll`, `srl`, `sra` and immediates |
| atomic memory operation (AMO) | 原子内存操作 | Slice S3 |
| load-reserved / store-conditional | 保留加载 / 条件存储 | LR/SC pair, bounded by Za64rs |
| reservation set | 保留集 | The 64-byte window LR/SC watches |
| fence | 屏障指令 | No-op in our simulator |
| fence.i | 指令屏障 | Instruction-stream synchronisation |
| bit manipulation | 位操作 | Zba/Zbb/Zbs, slice S4 |
| floating point | 浮点 | F/D, slices S6–S7 |
| half precision | 半精度 | Zfhmin conversions |
| rounding mode | 舍入模式 | `frm` field of `fcsr` |
| NaN boxing | NaN 装箱 | f32 values held in 64-bit registers |
| exception flag | 异常标志位 | `fflags`: NV, DZ, OF, UF, NX |

## 2. 特权与系统 / Privilege and system

| English | 中文 | Note in this project |
|---|---|---|
| hart | 硬件线程 | Single hart in this simulator |
| privilege mode | 特权模式 | M = 机器, S = 监督, U = 用户 |
| machine mode (M) | 机器模式 | Where traps land when not delegated |
| supervisor mode (S) | 监督模式 | Entered by `mret` in `rv64mi-p-illegal` |
| user mode (U) | 用户模式 | Where most test bodies run |
| control and status register (CSR) | 控制状态寄存器 | Keyed by number, e.g. `csr768_mstatus` |
| backing CSR | 后端寄存器 | `sstatus` writes log as `mstatus` |
| WARL | 写任意值读出合法值 | Field masking; we mirror Spike exactly |
| trap | 陷阱 | Umbrella for exceptions and interrupts |
| exception | 异常 | Synchronous, caused by an instruction |
| interrupt | 中断 | Asynchronous, e.g. `interrupt#1` (SSI) |
| cause code | 原因码 | Value in `mcause` |
| trap value | 陷阱值 | `mtval`: encoding, address, or fault PC |
| trap vector | 陷阱向量 | `mtvec`; direct or vectored |
| trap delegation | 陷阱委托 | `medeleg`/`mideleg`, zero throughout S1 |
| `mret` / `sret` | 特权返回指令 | Restore mode and PC from `m*`/`s*` registers |
| virtual memory | 虚拟内存 | Sv39, slice S8 |
| page table | 页表 | Swappable via `satp` |
| address translation | 地址翻译 | Page table walk |
| page fault | 缺页异常 | Trap cause 12/13/15 |
| A/D bits | 访问/脏位 | Updated by exception under Svade |
| `satp` | 地址翻译与控制寄存器 | Mode + ASID + root PPN |
| cache block | 缓存块 | 64 bytes (Zic64b) |
| cache management operation (CMO) | 缓存管理操作 | `cbo.clean/flush/inval/zero` |
| memory-mapped I/O | 内存映射 I/O | Devices reached by ordinary load/store |
| device tree (DTB) | 设备树 | Handed to firmware at boot |
| firmware | 固件 | OpenSBI-class M-mode runtime |
| boot | 引导 | M-mode init, delegate, hand off to S |
| counter | 计数器 | `cycle`, `time`, `instret` |
| performance counter | 性能计数器 | Zihpm, not implemented yet |
| trigger | 触发器 | `tselect`/`tdata*`; read-zero in S1 |
| breakpoint | 断点 | `ebreak`, trap cause 3 |
| HTIF / `tohost` | 主机-目标接口 / 退出握手地址 | A store here ends the run |

## 3. 验证 / Verification

| English | 中文 | Note in this project |
|---|---|---|
| differential testing | 差分测试 | Our whole method: same binary, both simulators |
| oracle / reference model | 参考模型（判定基准） | Spike at a pinned commit and flags |
| golden model | 黄金模型 | Synonym for the reference here |
| trace | 执行轨迹 | One record per retired instruction |
| retirement (retire) | 退休 | An instruction completes architecturally |
| commit log | 提交日志 | Spike's `--log-commits` output |
| normalization | 归一化 | The closed rewrite list in the freeze |
| first divergence | 首个分歧点 | Index, PC and both lines in the report |
| test set | 测试集 | 54 `rv64ui-p` + 17 `rv64mi-p` for S1; replaced “corpus” on 2026-09-26 |
| conformance | 一致性 / 符合性 | Claim against RVA22S64 |
| profile | 架构配置文件 | RVA22S64, RVA23S64 |
| pass criteria | 通过标准 | What "green" means, per slice |
| acceptance criteria | 验收标准 | What the architect signs off |
| evidence | 证据 | Commands run and their output |
| determinism | 确定性 | Same input, same trace, every run |
| reproducible | 可复现 | Anyone can rerun the measurement |
| regression | 回归 | A previously green slice turning red |
| coverage | 覆盖率 | What the test set does *not* reach |
| false negative | 漏报 | A bug the gate misses |
| false positive | 误报 | A failure that is not a defect |
| spin | 空转 / 自旋 | Spike's `write_tohost` loop after the halt store |
| halt | 停机 | Simulator exit when `tohost` turns nonzero |

## 4. 流程与协作 / Process and collaboration

| English | 中文 | Note in this project |
|---|---|---|
| architect | 架构师 | You: decides scope, approves methods |
| assistant | 助手 | Me: work orders, ADRs, integration, review |
| implementer | 实现者 | Writes `src/**` |
| verifier | 验证者 | Writes `tests/**` and the freeze |
| work order | 工作单 | `docs/work-orders/NNN-*.md` |
| architecture decision record (ADR) | 架构决策记录 | Append-only log, one decision per record |
| freeze | 冻结 | The approved observable surface |
| approval gate | 审批门 | Nothing is implemented before it opens |
| write zone | 写入区 | Who may edit which directory |
| baton | 交接棒 | Sequential handoff, one writer at a time |
| escalation | 上报 / 升级 | Stop after two failed attempts, bring evidence |
| scope | 范围 | What this slice does |
| non-goal | 非目标 | What we deliberately refuse |
| slice | 切片 | S1…S9 in the scope note |
| trade-off | 权衡 | What each decision costs |
| coupling / cohesion | 耦合 / 内聚 | Module-boundary quality |
| technical debt | 技术债 | Known, recorded, not hidden |
| definition of done | 完成定义 | Verifier runs the acceptance command |

## 5. 已弃用术语 / Retired terms

Keep the retired word here so a reader who searches for it lands on its replacement.

| Retired | 旧译法 | Use instead |
|---|---|---|
| corpus | 语料库 | **test set**（测试集）— “corpus” is linguistics vocabulary and hides that this is a fixed list of built binaries |
