# CC Message Capture

一个基于 Tauri v2 的 Claude API 消息抓包工具。通过本地 MITM（中间人）代理拦截 HTTPS 流量，捕获并展示 Claude API 的请求与响应内容。

## 技术栈

- **后端**: Rust + Tauri v2 + hudsucker（MITM 代理）
- **前端**: React 18 + Ant Design 5 + TypeScript
- **构建**: Vite + pnpm

## 功能

- HTTPS 中间人代理，透明拦截加密流量
- 可配置的 URL 过滤规则（支持通配符 `*`）
- 实时捕获请求/响应，推送至前端展示
- 请求详情查看：Headers、Body（自动格式化 JSON）
- 一键复制请求/响应内容
- 自动生成并管理 CA 根证书

## 环境要求

- [Rust](https://www.rust-lang.org/tools/install) >= 1.70
- [Node.js](https://nodejs.org/) >= 18
- [pnpm](https://pnpm.io/) >= 8
- macOS / Windows / Linux

## 快速开始

### 1. 安装依赖

```bash
cd cc-message-capture
pnpm install
```

### 2. 启动开发模式

```bash
pnpm tauri dev
```

首次启动会编译 Rust 依赖，耗时较长，后续启动会很快。

### 3. 构建生产版本

```bash
pnpm tauri build
```

## 使用指南

### 第一步：安装 CA 证书

应用启动后，需要先安装并信任 CA 根证书，才能解密 HTTPS 流量。

1. 点击工具栏的 **CA Cert** 按钮，获取证书文件路径
2. 证书位于 `~/.cc-message-capture/certs/ca_cert.pem`

**macOS 安装方式：**

```bash
# 方式一：双击证书文件，在「钥匙串访问」中打开
open ~/.cc-message-capture/certs/ca_cert.pem

# 方式二：命令行添加到系统钥匙串
sudo security add-trusted-cert -d -r trustRoot \
  -k /Library/Keychains/System.keychain \
  ~/.cc-message-capture/certs/ca_cert.pem
```

在「钥匙串访问」中找到 **CC Message Capture CA**，双击打开 → 展开「信任」→ 将「使用此证书时」改为「始终信任」。

**Windows 安装方式：**

```
1. 双击 ca_cert.pem 文件
2. 点击「安装证书」
3. 选择「本地计算机」→「将所有的证书都放入下列存储」
4. 点击「浏览」→ 选择「受信任的根证书颁发机构」
5. 确认安装
```

**Linux 安装方式：**

```bash
# Ubuntu / Debian
sudo cp ~/.cc-message-capture/certs/ca_cert.pem \
  /usr/local/share/ca-certificates/cc-message-capture.crt
sudo update-ca-certificates

# Fedora / RHEL
sudo cp ~/.cc-message-capture/certs/ca_cert.pem \
  /etc/pki/ca-trust/source/anchors/cc-message-capture.pem
sudo update-ca-trust
```

> **注意**: 浏览器（如 Chrome、Firefox）可能使用独立的证书存储，需要单独导入。

### 第二步：配置代理

将需要抓包的客户端 HTTP 代理指向本应用。默认代理地址：

```
127.0.0.1:9898
```

**方式一：系统代理（全局生效）**

macOS：系统设置 → 网络 → Wi-Fi → 代理 → 勾选「Web 代理(HTTP)」和「安全 Web 代理(HTTPS)」→ 填入 `127.0.0.1:9898`

命令行快捷设置：

```bash
# 开启
networksetup -setwebproxy Wi-Fi 127.0.0.1 9898
networksetup -setsecurewebproxy Wi-Fi 127.0.0.1 9898

# 关闭
networksetup -setwebproxystate Wi-Fi off
networksetup -setsecurewebproxystate Wi-Fi off
```

**方式二：环境变量（仅命令行程序）**

```bash
export http_proxy=http://127.0.0.1:9898 && export https_proxy=http://127.0.0.1:9898 && export no_proxy=localhost,127.0.0.1,::1

# 然后运行你的程序
curl https://api.anthropic.com/v1/messages ...
```

**方式三：应用内代理设置**

部分 Claude 客户端或 SDK 支持配置代理：

```python
# Python (anthropic SDK)
import httpx
client = anthropic.Anthropic(
    http_client=httpx.Client(proxy="http://127.0.0.1:9898")
)
```

```typescript
// Node.js (通过环境变量)
// 启动时设置: HTTPS_PROXY=http://127.0.0.1:9898 node app.js
```

### 第三步：配置过滤规则

在工具栏的 **Filters** 区域管理 URL 过滤规则：

- 默认过滤规则：`anthropic`（匹配所有包含 "anthropic" 的 URL）
- 点击输入框输入关键词，按 Enter 添加
- 点击标签上的 × 删除规则
- 支持通配符：`*claude*` 匹配包含 "claude" 的 URL

**常用过滤规则示例：**

| 规则 | 说明 |
|------|------|
| `anthropic` | 官方 API（api.anthropic.com） |
| `claude` | 包含 "claude" 的任意 URL |
| `openrouter` | OpenRouter 转发的请求 |
| `api.example.com` | 自定义 API 网关 |

> 过滤规则为空时，会捕获所有经过代理的请求。

### 第四步：开始抓包

1. 点击 **Start** 按钮启动代理
2. 状态显示为绿色 **Running**
3. 通过代理发送 Claude API 请求
4. 左侧列表实时显示捕获的消息
5. 点击某条消息，右侧面板展示详细的请求/响应内容

### 界面操作

| 操作 | 说明 |
|------|------|
| **Start / Stop** | 启动或停止代理服务 |
| **Port** | 代理监听端口（默认 9898，停止后可修改） |
| **CA Cert** | 显示 CA 证书路径 |
| **Clear** | 清空已捕获的消息列表 |
| **Request Tab** | 查看请求方法、URL、Headers、Body |
| **Response Tab** | 查看状态码、耗时、Headers、Body |
| **Copy 按钮** | 一键复制 Body 内容 |

## 项目结构

```
cc-message-capture/
├── src/                          # 前端代码
│   ├── main.tsx                  # 入口 + Antd 配置
│   ├── App.tsx                   # 根组件
│   ├── types/index.ts            # TypeScript 类型定义
│   └── views/Capture.tsx         # 主抓包页面
├── src-tauri/                    # Rust 后端
│   ├── src/
│   │   ├── main.rs               # 程序入口
│   │   ├── lib.rs                # Tauri Commands
│   │   ├── proxy.rs              # MITM 代理核心
│   │   └── cert.rs               # CA 证书管理
│   ├── Cargo.toml
│   └── tauri.conf.json
├── package.json
└── vite.config.ts
```

## 安全提示

- CA 证书拥有签发任意域名证书的能力，**使用完毕后请及时移除系统信任**
- 请勿将 `~/.cc-message-capture/certs/ca_key.pem` 私钥文件分享给他人
- 本工具仅用于开发调试目的，请勿用于未经授权的流量拦截

## 故障排除

**Q: 抓不到 HTTPS 请求？**
- 确认 CA 证书已安装并设为「始终信任」
- 确认代理配置正确（`127.0.0.1:9898`）
- 确认过滤规则匹配目标 URL

**Q: 启动代理报端口占用？**
- 修改 Port 为其他值（如 9899）
- 或关闭占用该端口的程序：`lsof -i :9898`

**Q: 证书需要重新生成？**
- 删除 `~/.cc-message-capture/certs/` 目录
- 重启应用，会自动生成新证书
- 需要重新安装信任新证书

## License

MIT
