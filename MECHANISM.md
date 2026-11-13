# 独立宏原型的实现契约

本文件对应 `ggpui-component-macro-probe`。该 workspace 可以独立编译运行，不依赖 GPUI，也没有接入主框架。本轮完成的是 Rust 界面选择、payload 更新与组件生命周期的原型闭环。它不能证明真实渲染性能或正式 UI IR 已完成。

## 两个宏各自负责什么

`#[component]` 把同步、非泛型自由函数包装成延迟初始化的组件工厂，生成同名命名 props 入口。初始化只执行一次。`use_state`、`use_store`、`use_command`、`use_context`、`use_memo` 在这次初始化中取得作用域句柄。

`view!` 解析界面结构，生成文本绑定、分支区域、列表区域。变化的是这些区域的依赖和数据，不是重新执行整个组件初始化。普通 Rust 函数体里的 `if`、`match`、`let-else` 仍是普通 Rust，宏不尝试重写整个语言。

## 单次选择与统一 payload

以这段代码为例：

```rust
match phase.get() {
    Phase::Live { done, total } | Phase::Cached { done, total }
        if done <= total => Progress(done: done, total: total),
    _ => Text("idle"),
}
```

实际展开保留一个原生 Rust match。source 只在该选择执行时求值一次，Rust 负责模式、guard、穷尽性、移动规则和候选顺序。选中的 arm 返回 `Selection::new(arm_id, payload, factory)`。

payload 包含该分支界面引用的绑定，是一个由嵌套二元组构成的完整值。例如 `(done, (total, ()))`。不用平坦元组，是为了避免标准库元组 `Clone` / `PartialEq` 实现的字段数量限制，14 个绑定也有真实测试。

同一 arm 更新只向一个 `Signal<Payload>` 写入一次。各个 `ReadProp` 从这个 signal 投影字段，不再重读 source，也不重新执行模式匹配和 guard。字段变为 ReadProp 后，界面体中的 `done.get()` 取得当前值；组件 props 可以直接传 `done`。条件和 guard 内的 `done` 仍是原生 Rust 数值。

“单次选择”不能解释成“所有 guard 加起来最多调用一次”。原生 Rust 的重叠或模式会在一个候选 guard 失败后尝试下一个候选，并再次执行 guard。原型使用同一个原生 match，保留这一语义，有计数测试。

源状态的同值写入不调度。一个批次中先移出再移回同一 arm，flush 时以最终值为准，不发生中间卸载。局部子组件的状态变化不会反过来让父选择订阅该状态。

## if let 与 let chains

支持 Rust 2024 的原生链条件：

```rust
if allowed.get()
    && let Some(user) = users.get()
    && user.enabled
    && let Some(quota) = user.quota
    && quota > minimum.get()
{
    Text(format!("{}:{}", user.get().name, quota.get()))
} else {
    Text("unavailable")
}
```

宏只收集当前链顶层 `let` 模式的绑定，不把条件内部闭包、子表达式里的局部名字误导出。短路执行遵守 Rust；运行时只订阅实际读取过的状态，因此前置条件为 false 时，后续 source 的改变不会唤醒此选择。

成功时，链的最终绑定一起放进 payload。链内后一个 `let` 可以使用前一个绑定，同名绑定以最后一个为准。else 分支不能引用成功分支的 let 绑定。`else if let` 形成嵌套持久区域，各层作用域独立。

## 模式语法与所有权

| 形式 | 原型契约 |
| --- | --- |
| 枚举、结构体、元组、数组解构 | 可用，使用 Rust 自身类型与穷尽性检查 |
| `A(x) \| B(x)` | 同一 arm、同一组件实例；所有候选必须绑定同名同类型 |
| `whole @ Pattern` | 可用，前提是 Rust 本身允许该所有权组合 |
| `code @ (1 \| 3..=7)` | 可用，范围/或模式按原生语义匹配 |
| `[first, rest @ ..]` | owned 数组可用；临时 Vec 的借用切片不能逃逸到持久 payload |
| `_`、`..`、字面量、限定常量/变体路径 | 可用 |
| `&Pattern` | 可以显式提取 Copy owned 值；不赋予临时引用持久生命周期 |
| `ref x`、`mut x` | 明确诊断拒绝，payload 是只读的 owned 投影 |
| `whole @ Some(name)`，其中 name 为 String | Rust 拒绝 whole 与子字段同时移动；宏不会偷偷克隆改变语义 |
| 模式位置的嵌套宏 | 明确拒绝，外层过程宏无法提前看到其绑定集合 |
| `|| let ...`、非 bool 链条件 | Rust 拒绝，宏不会定义不同含义 |

被界面持有的绑定要求 `Clone + PartialEq + 'static`。`'static` 约束阻止把 source 临时值的借用放进长期存在的闭包。不要求每一个只用于 guard 的局部值都满足 payload 约束。

过程宏在类型检查和名字解析之前运行。`syn::PatIdent` 不能判断 `FIRST` 是常量、局部绑定，还是一个无字段枚举变体。原型只导出界面引用的标识符，所以常见的 `None => Text("none")` 与不被界面引用的裸常量可以原样使用。界面还要引用同一个常量/变体时，应在模式和表达式里都写限定路径，例如 `constants::FIRST`、`Option::None`。裸常量重名的拒绝结果有诊断测试，不能声称任意 Rust 名字解析已被实现。

绑定引用收集是保守的 token 级收集，包含字符串占位符里的隐式格式捕获，保证 `format!("{name}")` 不冻结初始值。复杂名字遮蔽、自定义宏偷偷引入的捕获不作全语言分析。生成 payload 参数使用 mixed-site span，并对用户同名 `__probe_payload`、raw identifier 做了测试。

外部变量捕获仍遵守 Rust。Signal / ReadProp 是 Copy 句柄，适合放进重复执行的选择及子绑定。非 Copy 的 context 不能自动从 `Fn` 选择器中移动；可以先投影到 use_state / use_memo，或在源表达式块内显式 clone 为 payload。原型不会隐式克隆所有自由变量。

## Signal 定位和 relayout

runtime 对每个 Signal 保留 owner scope。`Engine::locate(signal)` 返回 runtime id、signal id、owner scope 和组件名；`Engine::dependents(location)` 根据 watcher 当前依赖集合反查 reactive 或 layout watcher，并返回 watcher id、scope、kind、label 和可选 layout node。

`View::layout(label, source)` 是一个无窗口的布局节点。挂载时先求值并记录 source 的 Signal 依赖，后续只在这些 Signal 变化时执行该节点的 relayout closure。runtime 为每个逻辑布局节点保存稳定的 `LayoutNodeId` 与持久 Taffy 0.15 `NodeId`，更新时调用 `set_style` 和 `compute_layout`，再将 Taffy Layout 转回原型 Rect。`Engine::relayout()` 会把活动 layout watcher 标脏，用于窗口尺寸、字体度量等不由 Signal 表达的外部变化。`Engine::layout()` 返回节点 label、node id 和最新 `Rect`。显式 relayout 不会写 Signal，也不会重新挂载组件。

这个原型的布局节点用 `Rect` 回调驱动 Taffy leaf，已经验证持久节点和计算缓存的接入，但未接入 Taffy 父子树、文本测量、真实 measure callback 或 GPU。生产实现可把 `LayoutNodeId` 扩展为持久父子树，并把 `DependencyLocation` 扩展为 dirty reason、layout boundary 和 measure cache 命中信息。

## 浏览器 Canvas 适配器

`web` crate 提供一个不改变 runtime 语义的 Rust/WASM 适配器。启动时它创建三个 `Signal<bool>`、三个 `View::layout` watcher 和对应的持久 Taffy leaf；Canvas click 事件只写入命中的 Signal，然后调用一次 `Engine::flush()`。runtime 根据 Signal 依赖定位并重跑对应 watcher，更新相同的 `LayoutNodeId` / Taffy `NodeId`，适配器再读取 `Engine::layout()` 绘制颜色、高度和边框。

这个例子刻意区分两层更新：布局计算已经是 Signal 驱动的局部 relayout，但当前 Canvas 适配器为了保持实现简单会在 flush 后重绘整张画布。它没有实现 GPU damage rect、Canvas dirty region、批量实例上传或 120Hz 动画调度，因此不能把示例的完整画布重绘性能当成局部 GPU 上传性能证据。浏览器交互和 WASM 编译由 `web/README.md` 中的命令及 CI wasm 检查覆盖。

## 生命周期和提交顺序

| 更新 | 行为 |
| --- | --- |
| arm 不变，payload 变化 | 复用分支和组件状态，一次发布完整 payload |
| 或模式从 A 变 B，但仍是同 arm | 复用相同状态 |
| 相同组件类型出现在不同 arm | 视为不同身份，默认卸载并重新挂载 |
| if-let 失败或匹配到其他 arm | 默认释放旧分支，旧句柄失效 |
| KeepAlive 隐藏分支 | 不释放局部状态，取消活动依赖；旧句柄报告 Inactive |
| KeepAlive 恢复 | 先写入最新 payload，再调度子树观察者 |
| 缓存超限 | 淘汰最久未使用的非活动分支，清理作用域 |
| 列表排序、同 key 新数据 | 保留该列表内同 key 实例，更新 item ReadProp |
| 删除再恢复 key、跨列表移动 | 创建新实例 |

新分支先在临时作用域完成构建，再替换当前分支。构建返回错误或 panic 时，BuildGuard 清理临时 scope、signal、watcher，当前输出仍存在。通过重复 key 构建失败和主动 panic 两种场景验证。

调度使用作用域深度和注册顺序。父选择先更新完整 payload，字段投影先于依赖它们的子 memo / text。测试中 `(1,2)` 更新到 `(3,6)`，子 memo 只观察两份完整值，不观察 `(3,2)`；多个 let 来源也验证了相同契约。

这仍不是任意 reactive graph 的通用无毛刺证明，也不是窗口级事务回滚。同 arm 后续用户 observer 报错、多个区域同时更新、自定义外部副作用和异步资源准备都需要正式运行时的提交协议。新分支组件初始化可能在旧分支 cleanup 前执行；不可把初始化副作用视为 post-commit effect。

## 验收与范围

统一入口是 `verify.ps1 -Offline`。它执行格式检查、workspace tests、Clippy `-D warnings`、原有八步演示、新增六步模式演示，再逐个输出错误示例诊断结果，并记录源码 SHA-256。

运行时测试分为原有 26 项、新增 34 项模式测试和 3 项布局测试。另有 10 项 compile-fail doctest，以及一个诊断驱动测试，逐一编译 13 份无效程序并核对错误码/消息。两份演示也包含运行断言。结果分别保存在 `results/tests.txt`、`results/diagnostics.txt`、`results/demo.txt`、`results/patterns-demo.txt`。

| 验收点 | 主要证据 |
| --- | --- |
| 只选择一次 | source / guard 计数与重叠或模式原生重试测试 |
| 全 payload 更新 | 多字段、多个 let、14 个绑定，子 memo 无新旧混合值 |
| 状态身份 | 同 arm、同 or-arm、跨 arm、key 排序/删除/跨列表 |
| 缓存激活 | 最新 payload + 原局部状态，嵌套缓存保持隐藏 |
| 清理 | 200 次新增 if-let 循环 + 原有 300 次循环；错误/panic 构建回收 |
| Rust 合法性 | 穷尽性、候选绑定一致性、移动/借用、chain 作用域及运算 |
| 开发者入口 | `#[component]` / `view!` / `use_*` / Store / Command 的真实程序 |
| 持久布局树 | `runtime/tests/layout.rs` 验证 Taffy 节点 id 在 Signal 更新后不变、卸载后从树移除；`.github/workflows/ci.yml` 验证 stable / 1.88 |

使用 Rust 2024，声明 MSRV 1.88，实际执行工具链 1.93.1。依赖固定在本 workspace 的 Cargo.lock。没有真实窗口、完整父子布局树、样式系统、异步 Command、跨线程 Store、渲染动画或 GPU 基准。正式接入 UI IR 与 GPUI 不在本轮修改范围内。
