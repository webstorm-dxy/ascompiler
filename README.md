# 问源语言编译器

这是一个用 Rust 编写的“问源”语言编译器。编译器会读取 `.as`
源文件，完成词法分析、语法分析、语义检查，随后通过 LLVM 生成目标文件，
并调用系统 C 编译器完成最终链接。

## 功能概览

- 支持中文关键字和中文标点变体的词法分析。
- 支持模块、导入、函数声明和入口点检查。
- 使用 `inkwell` 生成 LLVM IR 和本机目标文件。
- 链接 `runtime/std_io.c`，提供基础输入输出运行时。
- 支持通过 FFI 调用 Rust 编写的 `staticlib` 或 `cdylib`。
- 可输出 LLVM IR，便于调试代码生成结果。

## 目录结构

```text
Cargo.toml      workspace 清单
ascompiler/
  Cargo.toml    主编译器 crate，生成 asc 命令
  src/
    main.rs       命令行入口
    cli.rs        编译流程和链接流程
    lexer.rs      词法分析器
    parser.rs     AST 定义和语法分析器
    semantic.rs   模块、导入和可调用对象的语义检查
    codegen.rs    LLVM IR 代码生成
    stdlib.rs     标准库合并逻辑
wenyuan-ffi/
  src/          Rust FFI 辅助类型和导出宏
wenyuan-ffi-macros/
  src/          FFI 导出属性宏
runtime/
  std_io.c      链接到最终程序的 C 运行时
std/
  输入输出.as   问源标准库源码
demo/
  *.as          示例程序
examples/
  rust-ffi      可被问源调用的 Rust FFI 示例库
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
cargo run -- demo/condition.as --ir
```

编译示例程序为本机可执行文件：

```bash
cargo run -- demo/condition.as -o /tmp/wenyuan_condition
```

运行生成的程序：

```bash
/tmp/wenyuan_condition
```

入口函数需要使用 `@声明 入口` 标记，例如：

```text
@声明 入口
定义 方法 主（）返回 无：
。。
```

编译器命令行支持：

```bash
asc <源文件.as> [-o <输出文件>] [--ir] \
  [--ffi-lib <库路径>] [--ffi-search <目录>] [--ffi-rpath <目录>]
```

- `--ir`：只打印 LLVM IR，不链接可执行文件。
- `-o <输出文件>`：指定生成的可执行文件路径。
- `--ffi-lib <库路径>`：链接一个外部库文件，可重复传入。
- `--ffi-search <目录>`：添加库搜索路径，对应链接器的 `-L...`，可重复传入。
- `--ffi-rpath <目录>`：在 Unix-like 系统上写入动态库运行时搜索路径，可重复传入。

## Rust FFI

问源可以声明外部符号并在链接阶段接入 Rust 生成的 `staticlib` 或
`cdylib`。问源侧用 `@声明 外部("...")` 写出真实 native 符号名，然后用普通
方法签名描述参数和返回类型：

```text
#模块 Rust扩展

@声明 外部("wen_add")
定义 方法 相加（左：整数，右：整数）返回 整数

@声明 外部（"wen_print_text"）
定义 方法 打印（内容：字符串）返回 无
```

Rust 侧可以使用本仓库提供的 `wenyuan-ffi`。在 Rust FFI crate 的
`Cargo.toml` 中声明库类型：

```toml
[lib]
crate-type = ["staticlib", "cdylib"]

[dependencies]
wenyuan-ffi = { path = "../../wenyuan-ffi" }
```

然后用 `#[wenyuan_ffi::export(name = "...")]` 导出函数：

```rust
use wenyuan_ffi::AsStr;

#[wenyuan_ffi::export(name = "wen_add")]
fn add(left: i32, right: i32) -> i32 {
    left + right
}

#[wenyuan_ffi::export(name = "wen_text_len")]
fn text_len(text: AsStr) -> i32 {
    text.to_str().map(|value| value.chars().count() as i32).unwrap_or(0)
}

#[wenyuan_ffi::export(name = "wen_print_text")]
fn print_text(text: AsStr) {
    println!("{}", text.to_string_lossy());
}
```

v1 ABI 类型映射：

- `整数` ↔ `i32`
- `小数` ↔ `f64`
- `浮点` ↔ `f32`
- `布尔` ↔ `bool`
- `字符` ↔ `u8`
- `字符串` ↔ `wenyuan_ffi::AsStr` 或 `*const c_char`
- `无` ↔ `()`

构建并链接静态库示例：

```bash
cargo build -p wenyuan-rust-ffi-demo
cargo run -p ascompiler --bin asc -- demo/ffi.as \
  -o /tmp/wenyuan_ffi_demo \
  --ffi-lib target/debug/libwenyuan_rust_ffi_demo.a
```

运行：

```bash
/tmp/wenyuan_ffi_demo
```

输出应类似：

```text
20 + 22 = 42
文本长度 = 2
来自 Rust FFI
```

链接动态库时传入动态库文件，并在 Unix-like 系统上设置 rpath。macOS 示例：

```bash
cargo run -p ascompiler --bin asc -- demo/ffi.as \
  -o /tmp/wenyuan_ffi_demo_dyn \
  --ffi-lib target/debug/libwenyuan_rust_ffi_demo.dylib \
  --ffi-rpath target/debug
```

Linux 通常把动态库后缀改为 `.so`：

```bash
cargo run -p ascompiler --bin asc -- demo/ffi.as \
  -o /tmp/wenyuan_ffi_demo_dyn \
  --ffi-lib target/debug/libwenyuan_rust_ffi_demo.so \
  --ffi-rpath target/debug
```

Windows 可传入 `.dll` 的 import library 或工具链支持的动态库路径。

FFI v1 的限制：

- 动态库支持是链接期 shared library 链接，不是运行时 `dlopen`。
- 当前不把问源数组暴露给 Rust；后续可通过显式 slice ABI 扩展。
- 问源传给 Rust 的字符串是借用的 C 字符串；Rust 返回自有字符串暂不在 v1
  范围内。
- `@声明 外部` 不写符号名时仍保留旧行为：标准库函数使用内置映射，其他外部
  方法回退到编译器的 LLVM 名称清洗规则。

## 常用开发命令

```bash
cargo fmt
cargo test --workspace
cargo build
cargo build --release
cargo run -- demo/condition.as --ir
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
