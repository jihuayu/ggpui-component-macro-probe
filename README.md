# component / view 宏可行性原型

结论：可以实现。这里是真实的 Rust 过程宏和可执行的无窗口运行时，不是宏展开示意或伪代码。组件初始化一次，依赖变化更新对应文本/分支/列表，不重新运行根组件函数。

本目录是独立 Cargo workspace，使用自己的 Cargo.lock，不依赖 GPUI、不修改上层 Cargo.toml、不接入正在开发的渲染路径。只验证声明式控制流和组件状态机制，不能据此宣称完整样式系统、通用 UI IR 或独立 120 Hz 渲染已经完成。

本轮已实现 `if let`、Rust 2024 let chains、合法的 owned `@` 和或模式，并将 `if` / `match` 统一为“一次原生选择 + 一个完整 payload 更新”。实现契约、展开流程与验收矩阵见 [MECHANISM.md](MECHANISM.md)。这里完成的是该独立原型的选择与生命周期机制，仍有明确的 Rust 语法与运行时边界。

另外补上了 runtime 级的 Signal 定位和 headless relayout 原型。`Engine::locate(signal)` 返回 signal 所属 runtime、signal id、owner scope 和组件名；`Engine::dependents(location)` 返回当前依赖它的 reactive/layout watcher。`View::layout(label, || Rect { ... })` 会把读取到的 Signal 注册为布局依赖，Signal 更新只重跑相关布局 watcher，`Engine::relayout()` 用于窗口尺寸、字体度量等非 Signal 外部变化触发一次显式布局重跑。布局结果可由 `Engine::layout()` 读取。它验证的是依赖定位、持久 Taffy leaf 和局部重跑契约，仍不是完整窗口布局引擎。

当前 runtime 已接入持久的 Taffy 0.15 `TaffyTree<()>`。每个逻辑布局节点同时保存稳定的 `LayoutNodeId` 和 Taffy `NodeId`；Signal 更新会更新节点 style、执行 Taffy `compute_layout` 并回写 Rect，relayout 不会重新创建节点。卸载会从 Taffy 树移除节点。CI 位于 [.github/workflows/ci.yml](.github/workflows/ci.yml)，在 Linux 上运行 stable 与 Rust 1.88 两个工具链矩阵，并执行同一份 `verify.ps1 -Offline`。

## 浏览器 Canvas example

`web` crate 是一个独立的 Rust/WASM Canvas 适配示例。它把一个 Flex parent 和三个子节点接到 Signal 驱动的持久 Taffy 布局叶节点，点击 child、parent 或缩放按钮后只重跑相关 layout watcher；更新节点会短暂变成橙色，Canvas 适配层随后重绘画布，并在画面底部显示 Signal 写入、布局 watcher、relayout 和当前闪烁节点计数。节点底部同时显示稳定的逻辑 `LayoutNodeId` 与 Taffy `NodeId`。

从仓库根目录构建并启动静态服务器：

```powershell
$probeCargo = (rustup which cargo).Trim()
& $probeCargo build -p probe-web --target wasm32-unknown-unknown --release --locked
wasm-bindgen target/wasm32-unknown-unknown/release/probe_web.wasm `
  --target web `
  --out-dir web/pkg `
  --no-typescript
python -m http.server 8080 --directory web
```

打开 <http://127.0.0.1:8080/>，点击 child、parent 标题或 `− / +` 缩放控件即可观察父子 Flex 联动和局部更新提示。先安装一次与 `Cargo.lock` 对齐的 `wasm-bindgen-cli`：

```powershell
cargo install wasm-bindgen-cli --version 0.2.130
```

`web/pkg` 是生成产物并被 `.gitignore` 忽略。这个示例验证的是 runtime 的 Signal 定位、持久 Taffy 节点和局部 relayout；Canvas 目前仍整画布重绘，尚未实现 GPU damage rect 或局部 Canvas dirty region。

## 运行

在 PowerShell 中：

```powershell
cd D:\oss\ggpui-component-macro-probe
.\verify.ps1 -Offline
```

新机器未缓存依赖时省略 -Offline。脚本通过 `rustup which cargo` 找到实际工具链 Cargo，把产物放在本目录 target，保存测试、Clippy、演示日志和源码哈希到 results。当前机器另有自定义 cargo 前端，会重定向构建输出，因此复现脚本绕过该前端，不修改全局配置。

workspace 使用 Rust 2024 edition，声明最低 rust-version 为 1.88（let chains 的稳定版本）；本轮实际验证工具链为 rustc / Cargo 1.93.1，没有执行 1.88 工具链测试。

单独运行演示：

```powershell
$probeCargo = (rustup which cargo).Trim()
& $probeCargo run --locked --offline --target-dir target -p probe-demo
& $probeCargo run --locked --offline --target-dir target -p probe-demo --example patterns
```

演示是有断言的命令行场景，不会打开窗口。模拟界面事件的 Controls 只用于测试驱动；实际框架会由按钮、输入框等事件调用相同的状态/Command 句柄。

## 实际可编译写法

完整程序见 [demo/src/main.rs](demo/src/main.rs)，核心如下：

```rust
#[component]
fn ReportPage() -> View {
    let store = use_store::<ReportStore>();
    let expanded = use_state(|| true);
    let rows = store.select(|s| s.rows.clone());
    let phase = store.select(|s| s.phase.clone());

    view! {
        Column {
            if expanded.get() {
                Details()
            } else {
                Text("detail unmounted")
            },
            match phase.get() {
                Phase::Idle => Text("status: idle"),
                Phase::Ready { name } => Text(name),
            },
            for report in rows.get() key report.id {
                ReportRow(report: report)
            },
        }
    }
}
```

宏处理界面结构，运行时处理依赖和生命周期。宏不会把普通 Rust 函数中的每一个 if 都变成响应式控制流；初始化函数里的普通 get 是快照，持续派生值用 use_memo 或 view 内的表达式。

新示例 [demo/examples/patterns.rs](demo/examples/patterns.rs) 把 Store、Command、let chains、或模式、`@`、局部状态和缓存串起来。其中 `user` / `quota` 在条件里是普通 Rust 值，进入界面体后成为 `ReadProp`，通过 `.get()` 读取当前 payload：

```rust
view! {
    if let Some(user) = user.get()
        && let Some(quota) = user.quota
        && quota > 0
    {
        Text(format!("{}:{}", user.get().name, quota.get()))
    } else { Text("guest") },

    match phase.get() {
        Phase::Live { done, total } | Phase::Cached { done, total }
            if done <= total => Progress(done: done, total: total),
        whole @ Phase::Code(code @ (1 | 3..=7)) =>
            Text(format!("{}:{}", matches!(whole.get(), Phase::Code(_)), code.get())),
        _ => Text("idle"),
    },
}
```

所有模式绑定的集合构成同一个 payload 元组，仅包含界面引用的名字。guard 使用原生值，同一 arm 的界面只挂载一次，后续数据更新通过该元组发布；字段投影不再重新执行 source 或匹配原始枚举。

## 已验证的行为

60 项运行时测试（原有 26 项 + 本轮 34 项模式测试）、10 项 compile-fail doctest，以及一个逐一编译并核对原因的诊断测试（内含 13 个无效调用文件）。见 [behavior.rs](runtime/tests/behavior.rs)、[patterns.rs](runtime/tests/patterns.rs)、[compile_fail.md](runtime/compile_fail.md)、[diagnostics.rs](runtime/tests/diagnostics.rs)。日志见 [results/tests.txt](results/tests.txt)、[results/diagnostics.txt](results/diagnostics.txt)，两份带断言的演示见 [results/demo.txt](results/demo.txt) 与 [results/patterns-demo.txt](results/patterns-demo.txt)。

| 范围 | 证据 |
| --- | --- |
| 组件只初始化一次 | 修改局部状态、Store、match payload、列表顺序后，根组件 mount count 仍为 1 |
| hooks 和依赖 | use_state / use_memo / use_context / use_store / use_command；同值不调度；短路条件取消无用依赖；两个应用相互隔离 |
| if / if let / let chains | 多 let、布尔前缀/后缀、短路取消依赖、else-if、同名遮蔽；失败释放状态；同一 flush 前先关后开以最终值为准 |
| match / @ / 或模式 | 单次原生选择；同 arm 一次发布完整 payload、保留局部状态；或模式不同变体共用一个 arm；原生 guard 顺序和候选重试；范围、数组 rest、字段解构 |
| payload 一致性 | `(1, 2)` → `(3, 6)` 的子组件 memo 只观察这两个完整结果；多 let 也只发布完整元组；source 计数不随字段数增长 |
| 列表 key | 排序保留状态；同 key 新数据更新；删除/过滤后恢复重建；跨列表不共享状态；重复 key 在修改该列表前整批拒绝 |
| KeepAlive | 保留局部状态；隐藏期间不重算；恢复读取最新数据；LRU 条目限额；嵌套缓存不会误激活隐藏子分支 |
| Signal 定位与布局 | `locate` 能定位 owner scope；`dependents` 能列出 layout watcher；相关 Signal 更新只重跑局部 layout；稳定逻辑节点复用同一个 Taffy node；显式 `relayout` 不需要 Signal 写入；卸载后 Taffy 节点、布局结果和依赖消失 |
| 卸载与异常 | 旧句柄拒绝读写；初次挂载及新分支准备 panic 清理；新分支准备报错保留原分支；原有 300 次循环及新增 200 次 if-let 进出后 live scope/signal/watcher 数量不增长 |
| Rust 名字和嵌套 | `None`、限定常量、不被界面引用的裸常量、raw identifier、生成标识符卫生、隐式 format 捕获、嵌套 match、keyed 子树跟随 payload 更新 |
| 编译约束 | 非穷尽 match、或模式缺绑定/类型冲突、非法 @ 部分移动、临时借用逃逸、ref/mut、非法链运算/作用域、模式宏、不支持的 component 选项等；诊断文件核对错误码或明确消息 |

Clippy 使用 `-D warnings`，代码禁止 unsafe。生成的组件包装、命名 props 和投影都经过真实 Rust 类型检查；不是用字符串替换伪造结构。

## 关键实现发现

1. **单独加属性宏不够。** component 宏包装挂载入口与命名 props；view 宏把控制流转成 Switch/KeyedList 等持久区域；运行时负责真正的订阅、状态与清理。
2. **match 的值和分支身份必须分开。** Ready(A) → Ready(B) 只更新 payload。原生 match 返回 Selection { arm, payload, factory }，同 arm 一次写入完整元组，ReadProp 投影读取它，不重新匹配 source。一次选择不表示所有 guard 总计只调用一次；Rust 的或模式允许候选 guard 失败后再尝试其他候选。
3. **列表 item 同样要可更新。** key 负责实例身份；ReadProp 负责同 key 的新字段。仅保留旧闭包会产生“顺序正确、内容过时”的错误。
4. **调度顺序属于正确性。** 父控制区域必须先处理分支切换，再处理旧分支字段投影，否则新 enum 值可能使旧投影 panic。原型按作用域深度和注册顺序调度；这不是生产级通用拓扑调度证明。
5. **状态缓存与可见性不能混为一谈。** KeepAlive 要分开局部启用标记与祖先有效可见性，否则恢复外层缓存会误激活仍被内层隐藏的子组件。
6. **“构建一次”不等于表达式只执行一次。** 条件、投影、memo 会重新求值；首次组件初始化不需要重跑。

## 实现边界

- 支持 Text、Column/Row（无布局的逻辑分组）、`View::layout` headless 布局节点、命名组件 props、if/else/if-let/let chains、match/guard、owned @/或模式、带 key 的 for、KeepAlive(max_entries)。不支持样式链、真实事件控件、完整布局 DSL、图片、GPU 或动画；原设计文档的完整报告 UI 不能原样编译。`while`、`loop`、`let-else`、`?` 等可以用于普通 Rust 计算，不是本 DSL 的持久界面节点。
- component 当前要求同步、非泛型自由函数，返回具体 View。view 命名组件参数使用 ReadProp；普通静态 props、完整属性链、跨模块路径、依赖重命名、属性转发等宏工程能力尚未覆盖。
- 被界面引用的 payload 必须 Clone + PartialEq + 'static；支持合法 owned @，不自动克隆绕过 Rust 的 whole/field 部分移动规则。ref/mut 绑定明确拒绝，临时借用逃逸由 Rust 拒绝；显式引用模式提取 Copy 值可用。match 保留 Rust 穷尽性检查，不添加 wildcard。
- 过程宏没有 Rust 名字解析信息。未被界面引用的 `None` / 裸常量保持原生模式；界面同时引用裸常量/无字段变体时应在模式和表达式里都写限定路径（如 `constants::FIRST` / `Option::None`）。当前不会对所有复杂名字遮蔽做作用域分析，且拒绝模式位置的嵌套宏；不能宣称支持任意 Rust 模式宏。格式字符串的隐式捕获已覆盖，跨宏定义引入的隐藏绑定不在保证内。
- 闭包捕获遵循 Rust 所有权。推荐传递 Copy 的 Signal / ReadProp 句柄，外部非 Copy 值先放进 use_state / use_memo，或在选择表达式的块内显式克隆再移动给子闭包。原型不自动克隆任意自由变量。
- hooks 的语法检查仅覆盖宏能看到的调用；跨函数违规仍靠运行时作用域检查。宏无法静态判定所有用户函数是否纯；已拦截跟踪表达式中的 Signal 写入/Command，其他外部副作用仍需契约约束。
- source.set 后由测试显式 flush，模拟一次 UI 批次；没有真实事件循环、异步命令队列或跨线程 Store。Command 目前同步调用，只证明类型化依赖注入和状态反馈可与宏协作。
- provider 当前只在 Engine 根注册，没有子树覆盖、运行时替换或跨窗口生命周期。ReadStore 不是完整的生产版本化 Store。
- 重复 key 在该列表内做预检；这不等于整个窗口的通用事务回滚。新选择分支先准备再替换，准备报错或 panic 会清理临时作用域并保留原输出；同 arm 后续 observer 出错、用户外部副作用、多个区域合并提交仍没有全局回滚。生产版本需要草稿/提交边界和 post-commit effect。
- KeepAlive 限制的是非活动分支条目数，另有一个活动分支；没有字节预算、任务取消、焦点/IME/可访问性、退出动画、activation generation 或资源租约。
- scoped typed handles 在 UI 线程中使用，id 单调增长、销毁后失效；没有复用 slot 的 generation arena。生产实现需要补齐 epoch/generation 和资源回收策略。
- 原型采用 Vec 克隆、扫描依赖和全列表校验，没有衡量性能，不是大列表 O(1) 增量实现。它生成的是 UI 前端控制器与文本输出，不是可跨语言传输的正式 UI IR。

## 文件

- [macros/src/lib.rs](macros/src/lib.rs)：syn/quote 过程宏、DSL 解析和展开。
- [runtime/src/lib.rs](runtime/src/lib.rs)：作用域、依赖、只读 props、分支/列表和有限缓存。
- [runtime/tests/layout.rs](runtime/tests/layout.rs)：Signal 定位、布局依赖和 relayout 重跑测试。
- [demo/src/main.rs](demo/src/main.rs)：八步运行演示，最后断言 ReportPage 只挂载一次。
- [demo/examples/patterns.rs](demo/examples/patterns.rs)：六步模式演示，包含 source/guard 计数和状态保留断言。
- [MECHANISM.md](MECHANISM.md)：选择降级、payload 发布、生命周期和明确的语法契约。
- [verify.ps1](verify.ps1)：可复现检查。
- [results/source-manifest.json](results/source-manifest.json)：通过验证的输入文件哈希。

构建使用 Cargo.lock 中的 syn 2.0.119、quote 1.0.47、proc-macro2 1.0.107。解析方式使用 syn 的 ParseStream/AST，参考 [syn 解析接口](https://docs.rs/syn/2.0.119/syn/parse/index.html)。正式集成前应先将这些运行时契约移植到现有 LogicalUi/ReactiveUi，再扩充 view DSL；本原型没有修改那条实现路线。
