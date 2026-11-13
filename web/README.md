# Browser canvas example

这个 example 用 Rust/WASM 和浏览器原生 Canvas 直观展示 Signal 定位、持久 Taffy leaf 和局部 relayout：

- 画布包含 3 张卡片，每张卡片由一个独立 `Signal<bool>` 驱动。
- 点击某张卡片后，只切换该卡片的 Signal。
- 对应 layout watcher 重跑，卡片高度、颜色和边框改变。
- 画布底部显示 layout node 数、layout watcher 数、relayout 次数和 Signal 写入次数。
- 卡片底部显示稳定的逻辑 `LayoutNodeId` 和 Taffy `NodeId`。

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

打开 <http://127.0.0.1:8080/>，点击任意卡片。浏览器必须通过 HTTP 服务访问，不能直接双击 `index.html`，因为 ES module 和 WASM 的加载需要同源响应头。

`web/pkg` 是生成目录，已被 `.gitignore` 忽略，不提交构建产物。源码 example 可以通过：

```powershell
& $cargo check -p probe-web --target wasm32-unknown-unknown --locked --offline
```

CI 对 wasm target 做编译检查；浏览器交互需要本地运行上述 `wasm-bindgen` 和静态服务器命令。
