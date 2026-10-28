# Dogear · 折角

[English](README.en.md)

轻量、离线的 Windows 剪切板工具。复制过的都找得回：快捷键呼出，输入即搜索，回车即粘贴。

当前版本 0.1.6。历史、粘贴、截图和本机文字识别已经能用。技术栈是 Tauri 2、Vue 3、TypeScript 和 SQLite。设计说明见 [docs/design.md](docs/design.md)。以 [Apache-2.0](LICENSE) 许可发布。

## 它做什么

- 记住文字、链接、代码和图片。关掉窗口也不会丢，下次打开还在。
- 收藏一条后，它不会被条数上限清掉。清空历史时也会留下收藏。
- 记下复制来自哪个程序，可以在设置里隐藏，也可以忽略某些程序。
- 代码可以按格式化、压缩或原文查看。文字里的颜色会单独列出来。
- 图片用本机的 Windows 文字识别，不把内容发到网上。
- 界面可以在简体中文和英语之间切换。

剪切板正文只留在这台电脑上。默认不联网，也没有遥测。

## 安装

从 [Releases](https://github.com/AppleProPlusMax/Dogear/releases) 下载。

- `Dogear_0.1.6_x64_zh-CN.msi`：推荐。会覆盖同一产品的旧版，并卸掉以前用 setup.exe 装到别处的那一份。
- `Dogear_0.1.6_x64-setup.exe`：NSIS 安装包。

安装包没有代码签名。Windows 若提示风险，选择仍要运行。系统需要 WebView2，Windows 11 一般已经带有。

## 使用

从快捷方式或开始菜单打开时会显示窗口。开机自启只出现在托盘。缩小收到任务栏，关闭收到托盘。要退出，右键托盘图标，选退出。

| 操作 | 默认按键 |
| --- | --- |
| 显示或隐藏 | Alt+V |
| 选择 | ↑ ↓ |
| 粘贴 | 回车 |
| 纯文本粘贴 | Shift+回车 |
| 收藏 | Ctrl+P |
| 删除当前一条 | Delete |
| 截图 | Alt+C |
| 关闭窗口 | Esc |

呼出和截图的快捷键可以在设置里改。历史默认保留最近 200 条未收藏记录，也可以改成 500 或 1000。

## 从源码运行

需要 Node.js 20.19 或更高版本、Rust、MSVC 生成工具和 WebView2。

```
npm install
npm run tauri dev
```

文字识别样例：

```
cargo test probe_ocr_samples --manifest-path src-tauri/Cargo.toml -- --nocapture
```
