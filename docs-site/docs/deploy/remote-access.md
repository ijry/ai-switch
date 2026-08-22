---
title: 远程访问与 HTTPS
description: 通过 Tailscale 私网或 Funnel 公网访问 AI Switch，以及为本地算力池代理生成并信任自签根证书。本页给出实际的证书路径、逐平台信任命令和默认开关状态。
---

# 远程访问与 HTTPS

这一页涉及两件相互独立的事，先分清再往下读：

- **远程访问**：让别的设备连上你这台机器的 AI Switch 管理界面（Web 服务，默认端口 `3090`）。
- **本地算力池 HTTPS**：让本机的路由代理（默认端口 `19527`）以 `https://` 提供服务，给那些只接受 HTTPS 上游地址的客户端使用。

两者用的是不同的端口、不同的证书体系，不要混在一起。

## 远程访问的两条路

| 方式 | 可见范围 | 传输加密 | 适用场景 |
| --- | --- | --- | --- |
| Tailscale 私网 | 仅你自己 Tailscale 账号下的设备 | 由 Tailscale 提供 | 手机、笔记本访问家里那台常开的机器 |
| Tailscale Funnel | 公网任何人可访问该地址 | Tailscale 提供的 HTTPS | 确有必要对外提供访问时 |

两种方式都不改变 AI Switch 自身的鉴权：**访问令牌照样要带**。Tailscale 只解决"链路能不能通、通道是否加密"，不代替应用层的授权。

## Tailscale 私网

AI Switch 内置了一个 Go 编写的 sidecar（`ai-switch-tsnet`，基于 Tailscale 的 `tsnet` 库），它在应用进程之外以用户态方式加入你的 tailnet，不需要在系统里安装 Tailscale 客户端、也不需要管理员权限。

### 启用步骤

1. 在桌面端**设置 → Web 服务**里勾选**启用安全网络**。
2. **访问模式**选择**仅私网**。
3. 保存并启动服务。
4. 在**安全网络**区域点击**使用 OAuth 登录**，浏览器会打开 Tailscale 授权页面，完成授权后回到应用。

登录成功后界面会给出可用的访问地址，形如：

```text
http://100.x.y.z:3090
http://ai-switch.<your-tailnet>.ts.net:3090
```

前者是 tailnet 内网 IP，后者是 MagicDNS 名称。sidecar 默认使用主机名 `ai-switch`，状态数据保存在 `~/.ai-switch/tailscale/`。

### 另一种登录方式：授权密钥

如果不方便走浏览器 OAuth（比如远程 SSH 到一台没有图形界面的机器上），可以在同一个区域填入 Tailscale **授权密钥**并点击**使用授权密钥连接**。密钥会被持久化到 `~/.ai-switch/tailscale/auth-key`，以便重启后自动重连。

::: warning 授权密钥以明文保存
`~/.ai-switch/tailscale/auth-key` 是明文文件。用完后如果不需要自动重连，建议在 Tailscale 后台吊销该密钥，或改用 OAuth 登录。
:::

### 登录不是自动发生的

这一点值得强调：**应用启动时不会自动登录 Tailscale**。

启动时的恢复逻辑只在满足下列条件之一时才会尝试重连：本地存在已保存的授权密钥、或 sidecar 目录里存在此前持久化的 tsnet 登录状态。都不满足时，界面会显示"安全网络正在等待登录"，等你手动点登录。

也就是说，装上应用不等于加入了某个网络，你必须显式做一次授权动作。

## Tailscale Funnel（公网）

把**访问模式**改成**公网访问**后，sidecar 会用 Tailscale Funnel 监听，服务地址变成一个公网可达的 HTTPS 地址：

```text
https://ai-switch.<your-tailnet>.ts.net
```

Funnel 模式默认走 443 端口（443 / 8443 / 10000 这几个 Funnel 支持的端口会被保留使用）。要注意：

- **你的 Tailscale 账号必须允许 Funnel。** 这是 tailnet 的策略设置，在 Tailscale 后台开启。
- **HTTPS 由 Tailscale 提供**，证书由 Tailscale 签发和续期，你不需要自己准备证书。
- **访问令牌仍然生效。** 任何人都能连到这个地址，但没有令牌拿不到任何数据——令牌是这时唯一的门。
- **界面上的登录动作照旧。** 切到公网模式不会跳过 Tailscale 登录。

## 自备证书暴露 Web 服务

如果你不用 Tailscale，而是想直接把 Web 服务绑到一个非环回地址上，那就必须自己提供 TLS 证书，否则服务拒绝启动（错误码 `web.sensitive_transport_requires_tls`）。

- 桌面端：编辑 `~/.ai-switch/web-service.json`，填写 `tlsEnabled`、`tlsCertPath`、`tlsKeyPath`（界面上没有这三项的输入控件），然后重启 Web 服务。
- 独立服务器：设置环境变量 `AI_SWITCH_TLS_CERT_PATH` 与 `AI_SWITCH_TLS_KEY_PATH`。

两个路径必须同时提供，只给一个会以 `web.tls_paths_incomplete` 报错。详见 [Web 服务模式](/deploy/web-service) 和 [独立服务器](/deploy/standalone-server)。

## 本地算力池 HTTPS

这部分和远程访问无关，解决的是另一个问题：**某些客户端只接受 `https://` 的上游地址**，而 AI Switch 的本地路由代理默认是 `http://127.0.0.1:19527`。为此应用可以生成一套自签证书，让本地代理以 HTTPS 提供服务。

### 默认是关闭的

`route-proxy-https.json` 的默认值是 `enabled: false`、`autoStart: false`。也就是说：**除非你主动去开，本地 HTTPS 不会启用，证书也不会生成**。安装应用不会往你的系统信任库里写任何东西。

代理本身始终只绑定 `127.0.0.1`，端口从 `19527` 起，被占用时向上寻找下一个可用端口。启用 HTTPS 不会让它监听外部地址。

### 证书存在哪里

启用后，证书材料生成在 `~/.ai-switch/certs/route-proxy/`：

| 文件 | 内容 |
| --- | --- |
| `root-ca.pem` | 自签根证书，需要导入系统信任库的就是这个文件 |
| `root-ca-key.pem` | 根证书私钥（Unix 下权限 0600） |
| `server-cert.pem` | 代理实际使用的服务器证书 |
| `server-key.pem` | 服务器私钥（Unix 下权限 0600） |
| `metadata.json` | 根证书 SHA-256 指纹、SHA-1 指纹、有效期等元信息 |

证书参数：根证书 CN 为 `AI Switch Route Proxy Root CA`，有效期 3650 天；服务器证书 CN 为 `AI Switch Route Proxy localhost`，有效期 823 天，SAN 只包含 `localhost` 和 `127.0.0.1`。两张证书都倒签 1 天以容忍客户端时钟偏慢，所以服务器证书实际跨度为 824 天 —— 刻意留在 825 天以下，因为 macOS 和 iOS 对 TLS 服务器证书强制这一上限，自建根签发的证书同样受限。因为 SAN 里没有任何外部主机名，这套证书**只对本机访问有效**，拿去给别的机器用是不成立的。

### 怎么信任

在桌面端的 **本地算力池 HTTPS** 面板里操作，可用动作有：

- **生成并导入根证书**：一步完成生成证书材料 + 写入系统信任库。
- **重新导入根证书**：证书还在但信任状态丢了（比如系统信任库被清理过）时用。
- **重新生成证书**：吊销旧材料、生成新的一套。生成过程使用临时目录加原子替换，失败时回滚到 `.backup`，不会留下半成品。
- **卸载根证书**：从系统信任库移除，但保留文件。
- **删除本地证书材料**：彻底删除文件，前提是 HTTPS 已关闭且根证书已卸载。
- **打开证书目录**：在文件管理器里打开上面那个目录（桌面独占）。

面板同时显示根证书指纹、到期时间、证书目录和当前信任状态（已被系统信任 / 已被 NSS 信任 / 部分信任 / 未信任 / 未知）。

如果自动导入失败（常见于权限不足或非标准发行版），面板会给出**手动信任步骤**。实际使用的命令按平台不同：

```powershell
certutil.exe -user -addstore Root "$HOME\.ai-switch\certs\route-proxy\root-ca.pem"
```

写入的是**当前用户**的信任库（`-user`），不需要管理员权限。

macOS 写入登录钥匙串：

```bash
security add-trusted-cert -r trustRoot -k ~/Library/Keychains/login.keychain-db \
  ~/.ai-switch/certs/route-proxy/root-ca.pem
```

不带 `-d` 写入的是**用户**信任域，因此不需要管理员权限 —— 但 macOS 修改信任设置前仍会要求输入密码。取消那个弹窗会导致证书被导入却未被信任，浏览器随后报 `ERR_CERT_AUTHORITY_INVALID`。AI Switch 会把这种状态判为未信任，所以如果导入成功后信任状态仍显示未信任，请重新执行并完成弹窗验证。撤销时记得加 `-t` —— `security delete-certificate -Z <sha1> -t ~/Library/Keychains/login.keychain-db` —— 否则信任设置会比证书活得更久。

Linux 因发行版而异，按你的系统选一条：

```bash
# p11-kit（Arch、Fedora 等）
trust anchor ~/.ai-switch/certs/route-proxy/root-ca.pem

# Debian / Ubuntu
sudo install -Dm644 ~/.ai-switch/certs/route-proxy/root-ca.pem \
  /usr/local/share/ca-certificates/ai-switch-route-proxy-root-ca.crt
sudo update-ca-certificates

# RHEL / CentOS / Fedora
sudo install -Dm644 ~/.ai-switch/certs/route-proxy/root-ca.pem \
  /etc/pki/ca-trust/source/anchors/ai-switch-route-proxy-root-ca.pem
sudo update-ca-trust extract
```

Firefox 和部分基于 NSS 的应用不读系统信任库，需要单独导入：

```bash
certutil -A -d <nss-db-path> -n "AI Switch Route Proxy Root CA" -t C,, \
  -i ~/.ai-switch/certs/route-proxy/root-ca.pem
```

NSS 库里的昵称必须与根证书 CN 一致，也就是 `AI Switch Route Proxy Root CA`，卸载时按这个名字查找。

面板给出的手动步骤里路径是根据你机器上的实际位置生成的，直接照抄面板上的命令比照抄本页更可靠。

### 私钥不会外泄到界面

代理 HTTPS 的状态接口只返回证书目录、根证书路径、信任状态、到期时间和手动步骤，**不返回任何私钥内容**。

## 安全注意事项

::: warning 远程访问的几条底线
- **所有 `/api/*` 与 `/ws/events` 请求都需要访问令牌，走 Tailscale 也一样。** Tailscale 不是鉴权层，它只负责链路。
- **Tailscale 登录是手动动作。** 应用启动不会自动登录；只有在本地存有授权密钥或此前的 tsnet 状态时才会尝试重连。
- **绑 `0.0.0.0` 之前先想清楚。** 非环回监听必须配 TLS（否则拒绝启动），并且意味着这台机器所在网络里的任何设备都能碰到这个端口。
- **启用 Funnel 之前更要想清楚。** 那是一个公网地址，令牌是唯一的门。除非确有必要，优先用私网模式。
- **令牌等价于 shell 权限。** Web API 包含终端会话命令，令牌泄露的后果不止于配置被读取。
- **自签根证书只为本机服务。** 它的 SAN 只有 `localhost` 与 `127.0.0.1`；不要把根证书私钥拷到别的机器，也不要用它去为远程访问签发证书。
- **不再需要时记得清理。** 关闭本地 HTTPS 后，用面板的卸载与删除动作把根证书从系统信任库里移除，别让一个不再使用的根证书长期留在信任库中。
:::

## 下一步

- 回顾 Web 服务的启用步骤与接口，见 [Web 服务模式](/deploy/web-service)。
- 在服务器上部署无界面实例，见 [独立服务器](/deploy/standalone-server)。
- 了解本地代理如何路由请求，见 [协议路由与桥接](/guide/protocol-routing)。
- 遇到连不上的情况，先看 [FAQ](/faq)。
