# 问源语言编译器

这是一个用 Rust 编写的“问源”语言编译器。编译器会读取 `.as`
源文件，完成词法分析、语法分析、语义检查，随后通过 LLVM 生成目标文件，
并调用系统 C 编译器完成最终链接。

## 功能概览

- 支持中文关键字和中文标点变体的词法分析。
- 支持模块、导入、函数声明和入口点检查。
- 使用 `inkwell` 生成 LLVM IR 和本机目标文件。
- 链接 `runtime/std_io.c`，提供基础输入输出运行时。
- 可输出 LLVM IR，便于调试代码生成结果。

## 目录结构

```text
src/
  main.rs       命令行入口和完整编译流程
  lexer.rs      词法分析器
  parser.rs     AST 定义和语法分析器
  semantic.rs   模块、导入和可调用对象的语义检查
  codegen.rs    LLVM IR 代码生成
  stdlib.rs     标准库合并逻辑
runtime/
  std_io.c      链接到最终程序的 C 运行时
std/
  输入输出.as   问源标准库源码
demo/
  *.as          示例程序
```

## 编译要求

所有平台都需要安装：

- Rust 工具链，推荐使用 `rustup`。
- LLVM 20，必须和 `Cargo.toml` 中 `inkwell = { features = ["llvm20-1"] }`
  对应。
- C 编译工具链，命令行中需要能找到 `cc`。

本仓库的 `.cargo/config.toml` 默认写入了 macOS Homebrew on Apple Silicon
的 LLVM 路径：

```toml
[env]
LLVM_SYS_201_PREFIX = "/opt/homebrew/opt/llvm@20"
```

如果你的 LLVM 20 安装在其他位置，请修改这个文件，或在命令行中临时设置
`LLVM_SYS_201_PREFIX`。该变量应该指向 LLVM 20 的安装前缀，目录下通常包含
`bin/llvm-config`、`include/` 和 `lib/`。

## macOS 编译源码

### Apple Silicon

```bash
xcode-select --install
brew install rust llvm@20
export LLVM_SYS_201_PREFIX=/opt/homebrew/opt/llvm@20
cargo build
```

生成的编译器位于：

```bash
target/debug/asc
```

如果要构建发布版本：

```bash
cargo build --release
```

发布版本位于：

```bash
target/release/asc
```

### Intel Mac

Intel Mac 的 Homebrew 默认前缀通常是 `/usr/local`：

```bash
xcode-select --install
brew install rust llvm@20
export LLVM_SYS_201_PREFIX=/usr/local/opt/llvm@20
cargo build
```

如果你使用 `rustup` 安装 Rust，也可以只通过 Homebrew 安装 LLVM：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
brew install llvm@20
export LLVM_SYS_201_PREFIX=/usr/local/opt/llvm@20
cargo build
```

## Linux 编译源码

Linux 需要 Rust、LLVM 20 开发库以及系统 C 编译器。不同发行版的 LLVM 20
包名可能不同；关键是确认 `llvm-config --version` 能输出 `20.x`，并将
`LLVM_SYS_201_PREFIX` 指向对应安装前缀。

### Ubuntu / Debian

```bash
sudo apt update
sudo apt install -y build-essential curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

安装 LLVM 20 后，设置路径并编译：

```bash
export LLVM_SYS_201_PREFIX=/usr/lib/llvm-20
cargo build
```

如果系统仓库没有 LLVM 20，可以使用 LLVM 官方 apt 仓库或从源码安装 LLVM
20。安装完成后，确认：

```bash
$LLVM_SYS_201_PREFIX/bin/llvm-config --version
```

输出应为 `20.x`。

### Fedora

```bash
sudo dnf install -y gcc gcc-c++ make rust cargo llvm20 llvm20-devel
export LLVM_SYS_201_PREFIX=/usr/lib64/llvm20
cargo build
```

如果发行版使用不同路径，可以通过查找 `llvm-config` 来确认：

```bash
which llvm-config
llvm-config --prefix
```

然后把 `LLVM_SYS_201_PREFIX` 设置为 `llvm-config --prefix` 输出的路径。

### Arch Linux

```bash
sudo pacman -S --needed base-devel rust llvm
llvm-config --version
export LLVM_SYS_201_PREFIX=$(llvm-config --prefix)
cargo build
```

如果 Arch 仓库中的 LLVM 主版本不是 20，请安装 LLVM 20，或调整
`Cargo.toml` 中的 `inkwell` LLVM feature 以匹配本机 LLVM 版本。

## Windows 编译源码

推荐使用 MSYS2 或 Visual Studio Build Tools。无论选择哪种方式，都要确保：

- `cargo` 可用。
- LLVM 20 可用。
- `cc` 可用，因为编译器会调用 `cc` 链接最终可执行文件。

### MSYS2 UCRT64

1. 安装 MSYS2。
2. 打开 “MSYS2 UCRT64” 终端。
3. 安装依赖：

```bash
pacman -Syu
pacman -S --needed mingw-w64-ucrt-x86_64-rust \
  mingw-w64-ucrt-x86_64-gcc \
  mingw-w64-ucrt-x86_64-llvm
```

4. 确认 LLVM 版本：

```bash
llvm-config --version
```

5. 如果版本是 20.x，编译：

```bash
export LLVM_SYS_201_PREFIX=$(llvm-config --prefix)
cargo build
```

如果 MSYS2 当前提供的 LLVM 不是 20，需要安装 LLVM 20，或者让
`Cargo.toml` 的 `inkwell` feature 与本机 LLVM 主版本保持一致。

### Visual Studio Build Tools

1. 安装 Visual Studio Build Tools，并选择 C++ 桌面开发工具。
2. 安装 Rust：

```powershell
winget install Rustlang.Rustup
```

3. 安装 LLVM 20，并设置环境变量：

```powershell
setx LLVM_SYS_201_PREFIX "C:\Program Files\LLVM"
```

4. 打开新的 “Developer PowerShell for VS”，确认命令可用：

```powershell
cargo --version
clang --version
llvm-config --version
```

5. 编译：

```powershell
cargo build
```

如果 `cc` 不可用，可以优先使用 MSYS2 UCRT64；或者确保 LLVM/Clang 和
Visual Studio C++ 工具链都加入了 `PATH`。

## 运行示例

输出示例程序的 LLVM IR：

```bash
cargo run -- demo/test.as --ir
```

编译示例程序为本机可执行文件：

```bash
cargo run -- demo/test.as -o demo/test
```

运行生成的程序：

```bash
./demo/test
```

入口函数需要使用 `@声明 入口` 标记，例如：

```text
@声明 入口
定义 方法 主（）返回 无：
。。
```

## 常用开发命令

```bash
cargo fmt
cargo test
cargo build
cargo build --release
```

## 常见问题

### 找不到 LLVM

检查 `LLVM_SYS_201_PREFIX` 是否正确：

```bash
echo $LLVM_SYS_201_PREFIX
$LLVM_SYS_201_PREFIX/bin/llvm-config --version
```

如果版本不是 `20.x`，请安装 LLVM 20，或者同步调整 `Cargo.toml` 中的
`inkwell` feature。

### 链接失败或找不到 cc

安装系统 C 编译工具链：

- macOS: `xcode-select --install`
- Ubuntu / Debian: `sudo apt install build-essential`
- Fedora: `sudo dnf install gcc gcc-c++ make`
- Arch Linux: `sudo pacman -S base-devel`
- Windows: 使用 MSYS2 UCRT64 的 GCC，或安装 Visual Studio Build Tools

### macOS 上找到了错误版本的 LLVM

优先显式设置 LLVM 20 路径：

```bash
export LLVM_SYS_201_PREFIX=/opt/homebrew/opt/llvm@20
```

Intel Mac 通常使用：

```bash
export LLVM_SYS_201_PREFIX=/usr/local/opt/llvm@20
```

### 修改了 LLVM 安装路径后仍然失败

清理后重新编译：

```bash
cargo clean
cargo build
```
