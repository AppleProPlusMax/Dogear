# Dogear · 折角

轻量、离线的 Windows 剪切板工具。复制过的都找得回：快捷键呼出，输入即搜索，回车即粘贴。

技术栈：Tauri 2 + Vue 3 + TypeScript + SQLite。设计说明见 [docs/design.md](docs/design.md)。

当前阶段是原型验证（监听剪切板、Alt+V 呼出、回车粘贴、本机 OCR、窗口毛玻璃），还不是完整产品。

## 运行

需要 Node.js 20.19 或更高版本、Rust、MSVC 生成工具和 WebView2。

```
npm install
npm run tauri dev
```

窗口默认隐藏。Alt+V 呼出，上下键选择，回车粘贴到刚才的窗口，Esc 关闭。

识别样例图：

```
cargo test probe_ocr_samples --manifest-path src-tauri/Cargo.toml -- --nocapture
```
