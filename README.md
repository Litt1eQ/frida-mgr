# frida-mgr

一个面向 Android 的 Frida 项目管理工具：用 `uv` 创建/维护项目级 Python 虚拟环境（`.venv`），管理 `frida` / `frida-tools` 版本，并可下载/缓存/推送/启动 `frida-server`。

## 功能特性

- 项目级配置：在项目根目录生成 `frida.toml`
- Python 环境：使用 `uv` 创建 `.venv`，并在其中安装/升级 `frida` 与 `frida-tools`
- 版本管理：支持 `latest` / `stable` / `lts` 等别名；可刷新版本映射
- Android 设备管理：基于 `adb` 列设备、推送 `frida-server`、启动/停止/查看状态
- 便捷命令：`frida-mgr frida` / `ps` / `trace` / `run` / `top(fg)` 等在虚拟环境中运行

## 依赖与前置条件

- Rust 工具链（用于构建/安装 `frida-mgr`）
- `uv`（用于创建虚拟环境并安装 Python 包）
- `adb`（Android SDK Platform Tools）
- Android 设备（通常需要能通过某种“提权命令”启动 `frida-server`；默认使用 `su -c ...`，可在 `frida.toml` 里改）
- 网络访问（默认从 GitHub 下载 `frida-server` 与版本映射；如果用本地 `frida-server`，可减少下载需求）

## 安装

从源码安装：

```bash
cargo install --path .
```

或构建可执行文件：

```bash
cargo build --release
./target/release/frida-mgr --help
```

## 快速开始

1) 初始化项目（会创建 `frida.toml`、`.venv`，并安装 `frida`/`frida-tools`；默认会下载并缓存 `frida-server`）：

```bash
frida-mgr init
```

常用参数：

```bash
frida-mgr init --frida latest --python 3.11 --arch arm64
```

如果你想使用本地 `frida-server`（不从 GitHub 下载），并显式固定 `frida-tools` 版本：

```bash
frida-mgr init --server-source local --local-server-path ./bin/frida-server --frida-tools 13.3.0
```

2) 检查环境与设备：

```bash
frida-mgr doctor
frida-mgr devices
```

3) 推送并启动 `frida-server`：

```bash
frida-mgr push --start
```

4) 开始使用 Frida：

- 自动附加到前台应用（会自动选择设备与目标进程；别名：`fg`）

```bash
frida-mgr top -l agent.js -- -o out.txt
```

`top/fg` 会自动选择设备与目标（`-D/-p/-n` 等），不要额外传 `-U/-D/-H/-p/-n/-f/-F`；需要完全控制参数请用 `frida-mgr frida ...`。

- 完全手动调用 `frida`（等价于在项目虚拟环境中运行 `frida ...`）

```bash
frida-mgr frida -U -f com.example.app -l agent.js --no-pause
```

## 常用命令

- `frida-mgr init`：初始化项目（生成 `frida.toml` + `.venv`）
- `frida-mgr install <version|latest|stable|lts>`：切换/升级项目使用的 Frida 版本
- `frida-mgr sync [--recreate-venv] [--update-map]`：按 `frida.toml` 同步环境（Python 版本变更建议 `--recreate-venv`）
- `frida-mgr list`：列出可用的 Frida 版本（来自版本映射）
- `frida-mgr list --installed`：列出已缓存的 `frida-server` 版本
- `frida-mgr push [--device <id>] [--start]`：推送 `frida-server` 到设备（可选自动启动）
- `frida-mgr start|stop|status`：启动/停止/查看 `frida-server` 状态
- `frida-mgr run <cmd> -- <args...>`：在虚拟环境中运行任意命令
- `frida-mgr ps|trace`：在虚拟环境中运行 `frida-ps` / `frida-trace`
- `frida-mgr shell`：进入虚拟环境 shell
- `frida-mgr uv ...` / `frida-mgr pip ...`：透传调用 `uv` / `uv pip`（`pip` 会自动选择项目 `.venv` 的 Python）

## 配置文件（frida.toml）

`frida-mgr init` 会在项目根目录创建 `frida.toml`。常用字段示例：

```toml
[project]
name = "my-frida-project"

[python]
version = "3.11"
packages = ["ipython", "requests"]

[frida]
version = "16.6.6"
# tools_version = "13.3.0" # 可选：固定 frida-tools 版本

[android]
arch = "auto"              # auto/arm/arm64/x86/x86_64
server_name = "frida-server"
server_port = 27042
auto_start = false
root_command = "su"        # 会以 `${root_command} -c '...'` 执行

# 默认：下载并缓存 frida-server
[android.server]
source = "download"

# 使用本地 frida-server（与 source = "local" 配套）
# [android.server]
# source = "local"
# [android.server.local]
# path = "./bin/frida-server"
```

与推送相关的行为：

- 推送路径默认来自全局配置 `default_push_path`（默认 `/data/local/tmp/frida-server`）
- `default_push_path` 如果以 `/` 结尾，会被当作目录并自动拼接 `server_name`；否则当作完整文件路径
- 设备端日志默认写到 `${server_path}.log`（例如 `/data/local/tmp/frida-server.log`）

## 全局数据位置

`frida-mgr` 会在“系统配置目录”下保存一些全局数据（由 `directories` 库决定）；若无法获取系统目录，则回退到 `~/.frida-mgr/`。

- `config.toml`：全局配置（如 `adb_path`、默认推送路径等）
- `version-map.toml`：Frida ↔ frida-tools 版本映射（`sync --update-map` 可刷新）
- `cache/servers/`：缓存的 `frida-server`（按版本与架构分目录）

## 排错提示

- `uv` 或 `adb` 不可用：先运行 `frida-mgr doctor`，按提示安装或配置路径
- Python 版本变更导致 `.venv` 不匹配：运行 `frida-mgr sync --recreate-venv`
- `frida-server` 启动失败：检查设备是否允许执行、SELinux、以及 `root_command` 是否可用（需要支持 `-c`）；也可以尝试 `frida-mgr install <version>` 切换版本

## License

MIT（见 `LICENSE`）。
