use ascompiler::cli::{self, CompileOptions, FfiLinkOptions};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

#[derive(Debug, Clone, PartialEq, Eq)]
struct RustDependency {
    name: String,
    project_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectConfig {
    project_name: String,
    version: String,
    license: String,
    enable_chinese_paths: bool,
    exe_name: String,
    rust_deps: Vec<RustDependency>,
}

impl ProjectConfig {
    fn source_dir_name(&self) -> &'static str {
        if self.enable_chinese_paths {
            "源码"
        } else {
            "src"
        }
    }

    fn target_dir_name(&self) -> &'static str {
        if self.enable_chinese_paths {
            "目标输出"
        } else {
            "target"
        }
    }
}

pub fn run(args: Vec<String>) {
    if let Err(message) = run_result(args) {
        eprintln!("{}", message);
        process::exit(1);
    }
}

fn main() {
    run(std::env::args().collect());
}

fn run_result(args: Vec<String>) -> Result<(), String> {
    let Some(command) = args.get(1).map(String::as_str) else {
        return Err(usage().to_string());
    };

    match command {
        "new" => {
            let project_name = args
                .get(2)
                .ok_or_else(|| "错误: `salt new` 需要项目名".to_string())?;
            if args.len() > 3 {
                return Err(format!("错误: 未识别的参数 `{}`\n{}", args[3], usage()));
            }
            create_project(&PathBuf::from(project_name), project_name)
        }
        "init" => {
            if args.get(2).map(String::as_str) != Some("--bin") || args.len() > 3 {
                return Err("错误: 目前仅支持 `salt init --bin`".to_string());
            }
            let cwd =
                std::env::current_dir().map_err(|e| format!("错误: 无法读取当前目录: {}", e))?;
            let project_name = cwd
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("wenyuan-project")
                .to_string();
            create_project_in_existing_dir(&cwd, &project_name)
        }
        "build" => {
            ensure_no_extra_args(&args, 2)?;
            let cwd =
                std::env::current_dir().map_err(|e| format!("错误: 无法读取当前目录: {}", e))?;
            build_project(&cwd).map(|_| ())
        }
        "run" => {
            ensure_no_extra_args(&args, 2)?;
            let cwd =
                std::env::current_dir().map_err(|e| format!("错误: 无法读取当前目录: {}", e))?;
            let executable = build_project(&cwd)?;
            let status = Command::new(&executable)
                .status()
                .map_err(|e| format!("错误: 无法运行 {}: {}", executable.display(), e))?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("错误: 程序运行失败，退出状态 {}", status))
            }
        }
        "-h" | "--help" | "help" => {
            println!("{}", usage());
            Ok(())
        }
        other => Err(format!("错误: 未识别的命令 `{}`\n{}", other, usage())),
    }
}

fn ensure_no_extra_args(args: &[String], expected_len: usize) -> Result<(), String> {
    if args.len() == expected_len {
        Ok(())
    } else {
        Err(format!(
            "错误: 未识别的参数 `{}`\n{}",
            args[expected_len],
            usage()
        ))
    }
}

fn create_project(project_dir: &Path, project_name: &str) -> Result<(), String> {
    if project_dir.exists() {
        return Err(format!("错误: 目录已存在: {}", project_dir.display()));
    }
    fs::create_dir(project_dir)
        .map_err(|e| format!("错误: 无法创建目录 {}: {}", project_dir.display(), e))?;
    create_project_files(project_dir, project_name)?;
    println!("已创建问源可执行项目 {}", project_dir.display());
    Ok(())
}

fn create_project_in_existing_dir(project_dir: &Path, project_name: &str) -> Result<(), String> {
    fs::create_dir_all(project_dir)
        .map_err(|e| format!("错误: 无法创建目录 {}: {}", project_dir.display(), e))?;
    if find_config(project_dir).is_some() {
        return Err("错误: 当前目录已经包含 project.ascfg 或 项目设置".to_string());
    }
    create_project_files(project_dir, project_name)?;
    println!("已初始化问源可执行项目 {}", project_dir.display());
    Ok(())
}

fn create_project_files(project_dir: &Path, project_name: &str) -> Result<(), String> {
    let source_dir = project_dir.join("src");
    fs::create_dir_all(&source_dir)
        .map_err(|e| format!("错误: 无法创建源码目录 {}: {}", source_dir.display(), e))?;
    fs::write(
        project_dir.join("project.ascfg"),
        default_config(project_name),
    )
    .map_err(|e| format!("错误: 无法写入配置文件: {}", e))?;
    fs::write(
        source_dir.join("main.as"),
        default_main_source(project_name),
    )
    .map_err(|e| format!("错误: 无法写入入口源码: {}", e))?;
    Ok(())
}

fn build_project(project_dir: &Path) -> Result<PathBuf, String> {
    let config_path = find_config(project_dir).ok_or_else(|| {
        format!(
            "错误: 未找到项目配置文件，请在 {} 下创建 project.ascfg 或 项目设置",
            project_dir.display()
        )
    })?;
    let config_text = fs::read_to_string(&config_path)
        .map_err(|e| format!("错误: 无法读取配置文件 {}: {}", config_path.display(), e))?;
    let config = parse_project_config(&config_text)?;

    let source_dir = project_dir.join(config.source_dir_name());
    let source = read_project_sources(&source_dir)?;

    let target_dir = project_dir.join(config.target_dir_name());
    fs::create_dir_all(&target_dir)
        .map_err(|e| format!("错误: 无法创建输出目录 {}: {}", target_dir.display(), e))?;

    let mut output_path = target_dir.join(&config.exe_name);
    if cfg!(windows) {
        output_path.set_extension("exe");
    }

    let mut ffi = FfiLinkOptions::default();
    for dep in &config.rust_deps {
        let lib = build_rust_dependency(project_dir, dep)?;
        if let Some(parent) = lib.parent() {
            ffi.search_paths.push(parent.to_path_buf());
            if lib.extension().and_then(OsStr::to_str) != Some("a") {
                ffi.rpaths.push(parent.to_path_buf());
            }
        }
        ffi.libs.push(lib);
    }

    cli::compile_source(
        source,
        CompileOptions {
            input_path: source_dir,
            output_path: output_path.clone(),
            ir_mode: false,
            ffi,
        },
    );
    Ok(output_path)
}

fn build_rust_dependency(project_dir: &Path, dep: &RustDependency) -> Result<PathBuf, String> {
    let dep_dir =
        resolve_rust_dependency_path(project_dir, &dep.project_path).ok_or_else(|| {
            format!(
                "错误: Rust依赖 `{}` 的项目路径不存在或缺少 Cargo.toml: {}",
                dep.name,
                dep.project_path.display()
            )
        })?;

    let manifest_path = dep_dir.join("Cargo.toml");
    let status = Command::new(cargo_command())
        .arg("build")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .env("PATH", child_command_path())
        .status()
        .map_err(|e| format!("错误: 无法构建 Rust依赖 `{}`: {}", dep.name, e))?;
    if !status.success() {
        return Err(format!("错误: Rust依赖 `{}` 构建失败", dep.name));
    }

    infer_rust_library_path(&dep_dir).ok_or_else(|| {
        format!(
            "错误: Rust依赖 `{}` 构建完成，但未找到 staticlib/cdylib 产物",
            dep.name
        )
    })
}

fn child_command_path() -> String {
    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();

    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(".cargo/bin"));
    }
    paths.push(PathBuf::from("/usr/bin"));
    paths.push(PathBuf::from("/bin"));
    paths.push(PathBuf::from("/usr/sbin"));
    paths.push(PathBuf::from("/sbin"));
    paths.push(PathBuf::from("/opt/homebrew/bin"));
    paths.push(PathBuf::from("/usr/local/bin"));

    std::env::join_paths(paths)
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn cargo_command() -> PathBuf {
    let mut candidates = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".cargo/bin/cargo"));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/cargo"));
    candidates.push(PathBuf::from("/usr/local/bin/cargo"));
    candidates.push(PathBuf::from("/usr/bin/cargo"));

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("cargo"))
}

fn resolve_rust_dependency_path(project_dir: &Path, configured_path: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if configured_path.is_absolute() {
        candidates.push(configured_path.to_path_buf());
    } else {
        candidates.push(project_dir.join(configured_path));
        for ancestor in project_dir.ancestors() {
            candidates.push(ancestor.join(configured_path));
        }
    }

    candidates
        .into_iter()
        .find(|candidate| candidate.join("Cargo.toml").is_file())
}

fn infer_rust_library_path(dep_dir: &Path) -> Option<PathBuf> {
    let lib_name = read_rust_library_name(&dep_dir.join("Cargo.toml"))?;
    let mut target_dirs = Vec::new();
    target_dirs.push(dep_dir.join("target/debug"));
    for ancestor in dep_dir.ancestors() {
        target_dirs.push(ancestor.join("target/debug"));
    }

    let file_names = platform_library_file_names(&lib_name);
    for target_dir in target_dirs {
        for file_name in &file_names {
            let candidate = target_dir.join(file_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn read_rust_library_name(manifest_path: &Path) -> Option<String> {
    let text = fs::read_to_string(manifest_path).ok()?;
    let mut section = "";
    let mut package_name = None;
    let mut lib_name = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line.trim_matches(&['[', ']'][..]);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "name" {
            continue;
        }
        let value = unquote(value.trim());
        match section {
            "package" => package_name = Some(value),
            "lib" => lib_name = Some(value),
            _ => {}
        }
    }

    lib_name.or(package_name).map(|name| name.replace('-', "_"))
}

fn platform_library_file_names(lib_name: &str) -> Vec<String> {
    let mut names = vec![format!("lib{}.a", lib_name)];
    if cfg!(target_os = "macos") {
        names.push(format!("lib{}.dylib", lib_name));
    } else if cfg!(target_os = "windows") {
        names.push(format!("{}.dll", lib_name));
        names.push(format!("{}.lib", lib_name));
    } else {
        names.push(format!("lib{}.so", lib_name));
    }
    names
}

fn read_project_sources(source_dir: &Path) -> Result<String, String> {
    if !source_dir.is_dir() {
        return Err(format!("错误: 未找到源码目录 {}", source_dir.display()));
    }

    let mut files = Vec::new();
    collect_as_files(source_dir, &mut files)?;
    files.sort();

    if files.is_empty() {
        return Err(format!(
            "错误: 源码目录 {} 中没有 .as 文件",
            source_dir.display()
        ));
    }

    let mut source = String::new();
    for file in files {
        source.push_str(&format!("\n// 文件：{}\n", file.display()));
        source.push_str(
            &fs::read_to_string(&file)
                .map_err(|e| format!("错误: 无法读取源码文件 {}: {}", file.display(), e))?,
        );
        source.push('\n');
    }
    Ok(source)
}

fn collect_as_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in
        fs::read_dir(dir).map_err(|e| format!("错误: 无法读取源码目录 {}: {}", dir.display(), e))?
    {
        let entry = entry.map_err(|e| format!("错误: 无法读取源码目录项: {}", e))?;
        let path = entry.path();
        if path.is_dir() {
            collect_as_files(&path, files)?;
        } else if path.extension().and_then(OsStr::to_str) == Some("as") {
            files.push(path);
        }
    }
    Ok(())
}

fn find_config(project_dir: &Path) -> Option<PathBuf> {
    let ascii = project_dir.join("project.ascfg");
    if ascii.is_file() {
        return Some(ascii);
    }
    let chinese = project_dir.join("项目设置");
    if chinese.is_file() {
        return Some(chinese);
    }
    None
}

fn parse_project_config(text: &str) -> Result<ProjectConfig, String> {
    let mut config = ProjectConfig {
        project_name: String::new(),
        version: "0.1.0".to_string(),
        license: String::new(),
        enable_chinese_paths: false,
        exe_name: String::new(),
        rust_deps: Vec::new(),
    };
    let mut section = "";
    let mut current_dep: Option<RustDependency> = None;

    for raw_line in text.lines() {
        let line = strip_comment(raw_line).trim().replace(['“', '”'], "\"");
        if line.is_empty() {
            continue;
        }

        if line.ends_with('：') {
            if section == "Rust依赖" {
                if let Some(dep) = current_dep.take() {
                    config.rust_deps.push(dep);
                }
            }
            let name = line.trim_end_matches('：').trim();
            match name {
                "包配置" | "可执行文件设置" | "Rust依赖" => {
                    section = match name {
                        "包配置" => "包配置",
                        "可执行文件设置" => "可执行文件设置",
                        _ => "Rust依赖",
                    };
                }
                _ if section == "Rust依赖" => {
                    current_dep = Some(RustDependency {
                        name: name.to_string(),
                        project_path: PathBuf::new(),
                    });
                }
                _ => {}
            }
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("错误: 无法解析配置行 `{}`", raw_line.trim()));
        };
        let key = key.trim();
        let value = unquote(value.trim());

        match section {
            "包配置" => match key {
                "项目名" => config.project_name = value,
                "版本" => config.version = value,
                "许可证" => config.license = value,
                "启用系统级中文路径" => config.enable_chinese_paths = parse_bool(&value)?,
                _ => {}
            },
            "可执行文件设置" => {
                if key == "exe文件名" {
                    config.exe_name = value;
                }
            }
            "Rust依赖" => {
                if key == "项目路径" {
                    let dep = current_dep.as_mut().ok_or_else(|| {
                        "错误: `项目路径` 必须写在某个 Rust依赖 名称下面".to_string()
                    })?;
                    dep.project_path = PathBuf::from(value);
                }
            }
            _ => {}
        }
    }

    if let Some(dep) = current_dep {
        config.rust_deps.push(dep);
    }
    if config.project_name.is_empty() {
        return Err("错误: 配置文件缺少 `项目名`".to_string());
    }
    if config.exe_name.is_empty() {
        config.exe_name = config.project_name.clone();
    }
    for dep in &config.rust_deps {
        if dep.project_path.as_os_str().is_empty() {
            return Err(format!("错误: Rust依赖 `{}` 缺少 `项目路径`", dep.name));
        }
    }

    Ok(config)
}

fn strip_comment(line: &str) -> String {
    let mut in_string = false;
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0;
    while index + 1 < chars.len() {
        match chars[index] {
            '"' | '“' | '”' => in_string = !in_string,
            '/' if !in_string && chars[index + 1] == '/' => {
                return chars[..index].iter().collect();
            }
            _ => {}
        }
        index += 1;
    }
    line.to_string()
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "是" | "true" | "True" | "TRUE" => Ok(true),
        "否" | "false" | "False" | "FALSE" => Ok(false),
        other => Err(format!(
            "错误: 布尔值只能是 `是` 或 `否`，实际为 `{}`",
            other
        )),
    }
}

fn unquote(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('“')
        .trim_matches('”')
        .to_string()
}

fn default_config(project_name: &str) -> String {
    format!(
        "包配置：\n项目名 = “{}”\n版本 = “0.1.0”\n许可证 = “MIT”\n启用系统级中文路径 = 否\n\n可执行文件设置：\nexe文件名 = “{}”\n\nRust依赖：\n",
        project_name, project_name
    )
}

fn default_main_source(project_name: &str) -> String {
    format!(
        "引用 模块：标准库-输入输出-输出 为 输出\n\n@声明 入口\n定义 方法 主（）返回 无：\n    执行 输出：“你好，{}”\n。。\n",
        project_name
    )
}

fn usage() -> &'static str {
    "用法:\n  salt new <项目名>\n  salt init --bin\n  salt build\n  salt run"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_project_config_with_rust_dependency() {
        let config = parse_project_config(
            "包配置：\n项目名 = “示例项目”\n版本 = “2026.4.1”\n许可证 = “MIT”\n启用系统级中文路径 = 否\n\n可执行文件设置：\nexe文件名 = “demo”\n\nRust依赖：\n    外部依赖测试：\n        项目路径=“./examples/rust-ffi”\n",
        )
        .expect("config should parse");

        assert_eq!(config.project_name, "示例项目");
        assert_eq!(config.version, "2026.4.1");
        assert!(!config.enable_chinese_paths);
        assert_eq!(config.exe_name, "demo");
        assert_eq!(config.rust_deps.len(), 1);
        assert_eq!(config.rust_deps[0].name, "外部依赖测试");
        assert_eq!(
            config.rust_deps[0].project_path,
            PathBuf::from("./examples/rust-ffi")
        );
    }

    #[test]
    fn test_parse_project_config_defaults_exe_to_project_name() {
        let config = parse_project_config("包配置：\n项目名 = “测试”\n启用系统级中文路径 = 是\n")
            .expect("config should parse");

        assert_eq!(config.exe_name, "测试");
        assert_eq!(config.source_dir_name(), "源码");
        assert_eq!(config.target_dir_name(), "目标输出");
    }
}
