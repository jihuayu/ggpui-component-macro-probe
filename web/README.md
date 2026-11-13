# Browser canvas example

这个 example 用 Rust/WASM 和浏览器原生 Canvas 直观展示 Signal 定位、持久 Taffy leaf 和局部 relayout：

- 画布展示一个 Flex parent 和三个 keyed child，父节点驱动子节点的位置与尺寸。
- 每个 child 由独立 `Signal<bool>` 驱动，点击后只让相关 child 和父布局依赖进入更新路径。
- 更新后的 parent / child 区域会短暂变成橙色，直观看到本次重绘提示；底部显示当前闪烁节点。
- 父节点可以切换 `expanded / compact`，`− / +` 控件可以调整 Flex 容器缩放，两个操作都会联动布局。
- 画布底部显示 layout node 数、layout watcher 数、relayout 次数和 Signal 写入次数。
- 节点底部显示稳定的逻辑 `LayoutNodeId` 和 Taffy `NodeId`。

## 构建

先安装一次 wasm-bindgen CLI，版本与 Cargo.lock 中的 wasm-bindgen 对齐：

```powershell
cargo install wasm-bindgen-cli --version 0.2.130
```

从仓库根目录执行：

```powershell
$cargo = (rustup which cargo).Trim()
& $cargo build -p probe-web --target wasm32-unknown-unknown --release --locked
wasm-bindgen target/wasm32-unknown-unknown/release/probe_web.wasm `
  --target web `
  --out-dir web/pkg `
  --no-typescript
python -m http.server 8080 --directory web
```

打开 <http://127.0.0.1:8080/>，点击 child、parent 标题或缩放按钮。浏览器必须通过 HTTP 服务访问，不能直接双击 `index.html`，因为 ES module 和 WASM 的加载需要同源响应头。

`web/pkg` 是生成目录，已被 `.gitignore` 忽略，不提交构建产物。源码 example 可以通过：

```powershell
& $cargo check -p probe-web --target wasm32-unknown-unknown --locked --offline
```

CI 对 wasm target 做编译检查；浏览器交互需要本地运行上述 `wasm-bindgen` 和静态服务器命令。

当前 runtime 的 Taffy 原型仍以持久 leaf 为基本布局节点；示例中的 parent / child 关系由稳定 label、共享 Signal 依赖和 Flex 几何计算表达，Canvas 负责绘制连接线。真正的 Taffy parent-child 树、damage rect 和局部 GPU 上传仍属于后续 runtime 工作。
