<h1>
  <p align="center">
    <img src="./src-tauri/icons/128x128.png" width="128" alt=""><br />SuperManGa
  </p>
</h1>


<p align="center">
  <img alt="PC" src="https://img.shields.io/badge/PC-Windows%EF%BD%9CmacOS%EF%BD%9CLinux-orange">
  <img alt="格式" src="https://img.shields.io/badge/%E6%A0%BC%E5%BC%8F-文件夹%EF%BD%9Czip%EF%BD%9Ccbz%EF%BD%9Crar%EF%BD%9Ccbr%EF%BD%9C7z%EF%BD%9Cepub%EF%BD%9Cpdf%EF%BD%9Cmobi%EF%BD%9Cazw3%EF%BD%9Cazw-blue">
  <img alt="框架" src="https://img.shields.io/badge/%E6%A1%86%E6%9E%B6-Tauri_2_%2B_Vue_3-purple">
</p>

<p align="center">
  <strong>一个简单且纯粹的本地漫画阅读器</strong>
</p>
<p align="center">
单页 / 双页 / 条漫滚动 · 图像重采样 / 神经网络超分 / OCR 翻译
</p>


<p align="center">
  <img src="./screenshot.jpg" width="600">
</p>


<h3 align="center">
  中文 ｜ <a href="./README_en.md">English</a> ｜ <a href="./README_ja.md">日本語</a> ｜ <a href="./README_ko.md">한국어</a>
</h3>

## 项目背景

之前我主要使用 MangaMeeya 在电脑上看漫画，但它有几个缺陷：不支持 Unicode 文件路径、不支持播放动图、不支持阅读条漫，这让我十分困扰。

我也尝试过其他漫画阅读器，但始终没能找到满意的，大多数连基本的图像重采样功能都没有。

最终我决定自己写一个。

## 下载

按操作系统选择安装包 `v0.1.1`（也可前往 [Releases](https://github.com/ssnangua/SuperManGa/releases) 查看全部版本与更新说明）：

| 系统 | 下载 |
| --- | --- |
| Windows | [![便携版 x64](https://img.shields.io/badge/便携版-x64-blue?logo=data:image/svg%2bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI%2BPHBhdGggZmlsbD0iI2ZmZiIgZD0iTTAgMy40NDkgOS43NSAyLjF2OS40NTFIMG0xMC45NDktOS42MDJMMjQgMHYxMS40SDEwLjk0OU0wIDEyLjZoOS43NXY5LjQ1MUwwIDIwLjY5OU0xMC45NDkgMTIuNkgyNFYyNGwtMTIuOS0xLjgwMSIvPjwvc3ZnPg%3D%3D)](https://github.com/ssnangua/SuperManGa/releases/download/v0.1.1/SuperManGa-0.1.1-windows-x64.zip) |
| macOS | [![DMG Apple Silicon](https://img.shields.io/badge/DMG-Apple%20Silicon-black?logo=apple&logoColor=white)](https://github.com/ssnangua/SuperManGa/releases/download/v0.1.1/SuperManGa-0.1.1-macos-arm64.dmg) [![DMG Intel x64](https://img.shields.io/badge/DMG-Intel%20x64-black?logo=apple&logoColor=white)](https://github.com/ssnangua/SuperManGa/releases/download/v0.1.1/SuperManGa-0.1.1-macos-x64.dmg) |
| Linux | [![deb arm64](https://img.shields.io/badge/deb-arm64-A80030?logo=debian&logoColor=white)](https://github.com/ssnangua/SuperManGa/releases/download/v0.1.1/SuperManGa-0.1.1-linux-arm64.deb) [![deb x64](https://img.shields.io/badge/deb-x64-A80030?logo=debian&logoColor=white)](https://github.com/ssnangua/SuperManGa/releases/download/v0.1.1/SuperManGa-0.1.1-linux-x64.deb) |

> [!NOTE]
> 运行环境 / 超分模型 / 文字检测器模型 / OCR 模型 **没有打进安装包**，要使用神经网络超分 / OCR 翻译功能，需在应用内自行下载。

## 功能特性

|  | 功能 | 说明 |
| :---: | --- | --- |
| <img src="./src/assets/folder_open.svg" width="18" height="18" alt=""> | 本地漫画阅读 | 打开文件夹、压缩包或电子书 |
| <img src="./src/assets/mode_double_pages.svg" width="18" height="18" alt=""> | 单页 / 双页 | 页式翻页，适合传统漫画 |
| <img src="./src/assets/mode_infinite_scrolling.svg" width="18" height="18" alt=""> | **条漫滚动** | 纵向连续滚动，适合条漫 |
| <img src="./src/assets/direction_right_to_left.svg" width="18" height="18" alt=""> | 开页方向 | 左开 / 右开（日漫通常为右开） |
| <img src="./src/assets/fit_contain.svg" width="18" height="18" alt=""> | 缩放模式 | 适应窗口 / 原尺寸 / 定宽 / 缩放 |
| <img src="./src/assets/play.svg" width="18" height="18" alt=""> | 播放 | 定时滚动 / 翻页 |
| <img src="./src/assets/thumb.svg" width="18" height="18" alt=""> | 缩略图视图 | 缩略图网格显示所有图片 |
| <img src="./src/assets/minimalist_view_enter.svg" width="18" height="18" alt=""> | 极简视图 | **专注阅读**，阅读区撑满窗口 |
| <img src="./src/assets/fullscreen_enter.svg" width="18" height="18" alt=""> | 全屏 | **沉浸式阅读** |
|  | 图像重采样 | 线性插补 / 双三次插补 / Lanczos / 平均像素法 + 锐化 |
|  | Anime4K | GPU 着色器增强，偏锐化 / 去糊 |
| <img src="./src/assets/enhance.svg" width="18" height="18" alt=""> | **神经网络超分** | `MangaJaNai` / `Real-ESRGAN` |
| <img src="./src/assets/translate.svg" width="18" height="18" alt=""> | **OCR 翻译** | 文字检测 → OCR → 在线 / AI 翻译 |
| <img src="./src/assets/palette.svg" width="18" height="18" alt=""> | 色彩调节 | 亮度 / 对比度 / 饱和度 |
| <img src="./src/assets/lens.svg" width="18" height="18" alt=""> | 放大镜 | 局部放大查看细节 |
| <img src="./src/assets/bookmark.svg" width="18" height="18" alt=""> | 书签 | 可添加带备注的书签 |
|  | 章节识别 | 按文件名规则切章，支持自定义正则 |
|  | 最近打开记录 | 自动记住最近打开的文件与阅读进度 |
| <img src="./src/assets/language.svg" width="18" height="18" alt=""> | 多语言 | 简中 / 繁中 / 英 / 日 / 韩 |
| <img src="./src/assets/theme_dark.svg" width="18" height="18" alt=""> | 主题 | 明亮 / 暗黑 |

### 支持的格式

| 类型 | 扩展名 / 说明 |
| --- | --- |
| 文件夹 | 直接按图片顺序阅读；可递归子目录 |
| 压缩包 | `.zip` / `.cbz` / `.rar` / `.cbr` / `.7z` |
| 电子书 | `.epub` / `.pdf` / `.mobi` / `.azw3` / `.azw` |

> [!NOTE]
>
> - RAR / CBR / 7z 等会解压到本地缓存
> - PDF 会栅格化到本地缓存；加密的 PDF 会弹密码框
> - 不支持 KFX / 有 DRM 的 Kindle 电子书

### 神经网络超分

| 类型 | 说明 |
| --- | --- |
| 运行环境 | `CPU` / `CUDA 12` / `CUDA 13`（有匹配 `CUDA` 包时优先 GPU） |
| 超分模型 | `MangaJaNai 2x` / `Real-ESRGAN Anime 2x` |

> [!NOTE]
> 运行环境**不含**完整 `CUDA` / `cuDNN`，本机需自行装好对应工具包。

### 文字检测 → OCR → 翻译

通过文字检测器检测图片中的文字区域，将检测结果交给 OCR 引擎识别。

| 类型 | 选项 |
| --- | --- |
| 运行环境 | `CPU` / `CUDA 12` / `CUDA 13`（有匹配 `CUDA` 包时优先 GPU） |
| 文字检测器 | `DBNet` / `CTD` / `YOLO` / `SaberYOLO` |
| OCR 引擎 | `系统 OCR`（仅 Windows） / `Manga OCR` / `PaddleOCR` / `MIT 48px` / `EasyOCR` |

支持以下翻译服务：

| 服务商 | 说明 |
| --- | --- |
| **AI 翻译** | OpenAI 兼容接口，漫画提示词批量翻译 |
| 微软翻译 | Edge 免费通道，无需配置 |
| Google 翻译 | 免费网页接口，无需配置 |
| Yandex 翻译 | 免费通道，无需配置 |
| DeepL | 官方 API，需要 API Key |
| 彩云小译 | 需要 Token |
| 百度翻译 | 官方开放平台，需要 APP ID 与密钥 |
| 有道翻译 | 官方开放平台，需要应用 ID 与密钥 |
| 腾讯翻译 | 腾讯云机器翻译，需要 SecretId / SecretKey |
| 火山翻译 | 火山引擎机器翻译，需要 Access Key |
| 阿里翻译 | 阿里云机器翻译，需要 AccessKey |

API 密钥加密保存在系统凭据库（失败则回退到文件加密），**不会**明文写入普通配置文件。

可调字体、字号、横竖排、对齐、描边与自动配色等叠加样式。

<details>
<summary>AI 翻译预设服务商</summary>

| 服务商 | 默认接口地址 |
| --- | --- |
| 本地 LM Studio | `http://127.0.0.1:1234/v1` |
| 本地 Ollama（OpenAI 兼容） | `http://127.0.0.1:11434/v1` |
| DeepSeek | `https://api.deepseek.com/v1` |
| 阿里云通义（DashScope） | `https://dashscope.aliyuncs.com/compatible-mode/v1` |
| 智谱 GLM | `https://open.bigmodel.cn/api/paas/v4` |
| Moonshot（Kimi） | `https://api.moonshot.cn/v1` |
| 硅基流动 | `https://api.siliconflow.cn/v1` |
| Agnes AI | `https://apihub.agnes-ai.com/v1` |
| MiniMax | `https://api.minimaxi.com/v1` |
| 小米 MiMo | `https://api.xiaomimimo.com/v1` |
| OpenAI | `https://api.openai.com/v1` |
| OpenRouter | `https://openrouter.ai/api/v1` |
| Google Gemini（OpenAI 兼容） | `https://generativelanguage.googleapis.com/v1beta/openai` |
| _自定义 OpenAI 兼容服务_ | _（手动输入接口地址）_ |

</details>

### 应用数据

Windows 下，首次运行会在可执行文件目录创建 `SuperManGaData/`，用于存放应用数据与缓存文件（如果该位置不可写，则回退到系统应用数据目录）。

## 常见问题

<details>
<summary>Windows 便携版提示缺少 WebView2</summary>

> 阅读器依赖 **WebView2 Evergreen**。Win10/11 通常已自带；若缺失，按提示安装微软运行库后再启动。

</details>

<details>
<summary>打开 EPUB / MOBI 没有内容</summary>

> 本应用只能阅读电子书里的**图片**，纯文字的电子书会没有内容；<br />

</details>

## 其他

- [开发文档](./DOCS_zh.md)

## 相关

- 读取 RAR / CBR 时使用了 [unrar](https://www.rarlab.com/) 官方解压库。UnRAR 许可与本项目代码分开，说明见 [RARLAB](https://www.rarlab.com/license.htm)
- PDF 渲染依赖 [PDFium](https://pdfium.googlesource.com/pdfium/)
- 神经网络超分基于 [ONNX Runtime](https://onnxruntime.ai/)
- 文字检测 / OCR / 翻译流程参考了一些漫画翻译工具（如 [Saber-Translator](https://github.com/MashiroSaber03/Saber-Translator) / [manga-image-translator](https://github.com/zyddnys/manga-image-translator) 等）
