# 插件 v0.1.7 验证记录

## 错误依据与修复范围

用户提供的错误列表显示 YouTube 页面拒绝加载固定扩展 ID 的 `icons/32.png`，堆栈仍指向旧 `floating.js` 函数和远超当前清理脚本长度的列号。说明至少存在更新前脚本或保留的历史记录，不将截图中的六条记录夸大成六类独立故障。

原 PNG 字节由 esbuild dataurl loader 内置，脚本用 Blob / createImageBitmap 解码并绘制 Canvas。页面 DOM 不再包含插件 Logo 图片 URL；工具栏和插件自身页面仍使用其原始图标文件。取消了不再需要的图片 web_accessible_resources 声明，没有新增网页权限。

生命周期管理在扩展上下文失效时主动停止扫描、定时器、IdleCallback、MutationObserver、ResizeObserver、页面事件及 storage 监听，清理已有卡片控件。此行为适用于本版脚本；已运行的更旧版本仍需刷新网页替换。

## 测试

- 前端：15 个文件、119 项断言通过。桌面 Rust / 安装器 / 下载内核没有修改，本次不重新构建桌面安装包。
- 新增 `tests/extension-lifecycle.browser.mjs`，在隔离 Chromium 中使用实际 MV3 构建。受控 YouTube 页面设置 `img-src 'none'` 与禁止页面脚本策略，下载入口仍显示。
- 比较封闭 Shadow DOM 内 Canvas 的 32×32 RGBA SHA256 与插件自身页面读取原始 PNG 后解码的 RGBA SHA256，相等；确认页面没有 `chrome-extension://` 图片资源请求。
- 通过测试扩展的真实 `chrome.runtime.reload()` 使旧上下文失效。旧页面控件被清理；再执行 SPA 导航、30 次 DOM 变化、resize 与 scroll 并等待，旧控件不再创建。页面 console、pageerror 和浏览器 Log 错误均为空。测试浏览器对 unpacked runtime reload 的重新启用行为不计为用户页面重载恢复验证。
- B站左上角、抖音操作栏／备用位置、图标居中测试通过；嵌套滚动、变换、重复卡片、裁切、对话框与样式恢复回归通过。
- 抖音主页、背景封面、搜索、推荐／精选、旧票据拒绝、实际浏览器视频与音频解码回归通过。B站预览菜单和实际音视频下载回归通过。

测试使用预授予可选权限的独立测试清单、受控网络响应，不读取用户浏览器资料或更改其权限，也不宣称真实所有 YouTube 页面全部验证。新测试复现命令：构建插件、准备 Playwright Chromium，然后执行 `node tests/extension-lifecycle.browser.mjs`。

## 包

`SnowDown_BrowserExtension_0.1.7.zip`：203225 字节，SHA256 `fd0f0e389fcb93398a7d78847adf0e151e4f7126e235bba7da5abb9769ee26a3`。共 13 个公开文件。更新本机已准备插件目录及常用运行目录内置插件资源，桌面程序保持 v0.2.10。GitHub 使用独立标签 `extension-v0.1.7` 和 `make_latest=false`，避免覆盖桌面更新入口。
