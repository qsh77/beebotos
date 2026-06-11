#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{anyhow, Context};
use beebotos_launcher::{
    parse_launcher_command, read_env_file, write_env_file, LauncherCommand, WEB_CONSOLE_URL,
};

const RUNNER_SCRIPT: &str = "beebotos-run.ps1";

fn main() {
    let command = parse_launcher_command(std::env::args().skip(1));
    if let Err(err) = run(command) {
        show_error("BeeBotOS Launcher", &err.to_string());
    }
}

fn run(command: LauncherCommand) -> anyhow::Result<()> {
    let root = app_root()?;
    match command {
        LauncherCommand::Ui => run_ui(root),
        LauncherCommand::Start => {
            ensure_runtime_env(&root)?;
            run_runner(&root, "start")?;
            Ok(())
        }
        LauncherCommand::Stop => {
            run_runner(&root, "stop")?;
            Ok(())
        }
        LauncherCommand::Restart => {
            ensure_runtime_env(&root)?;
            run_runner(&root, "restart")?;
            Ok(())
        }
        LauncherCommand::Status => {
            let output = run_runner(&root, "status")?;
            show_info("BeeBotOS 状态", &output);
            Ok(())
        }
        LauncherCommand::OpenWeb => open::that(WEB_CONSOLE_URL).context("打开 Web 控制台失败"),
        LauncherCommand::OpenLogs => open_logs(&root),
    }
}

fn app_root() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("读取 launcher 路径失败")?;
    Ok(exe
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(".")))
}

fn logs_path(root: &Path) -> PathBuf {
    root.join("data").join("logs")
}

fn runner_path(root: &Path) -> PathBuf {
    root.join(RUNNER_SCRIPT)
}

fn ensure_runtime_env(root: &Path) -> anyhow::Result<()> {
    let path = root.join(".env");
    let config = read_env_file(&path).context("读取 .env 失败")?;
    write_env_file(&path, &config).context("准备 .env 失败")
}

fn open_logs(root: &Path) -> anyhow::Result<()> {
    let path = logs_path(root);
    std::fs::create_dir_all(&path).context("创建日志目录失败")?;
    open::that(path).context("打开日志目录失败")
}

fn run_runner(root: &Path, action: &str) -> anyhow::Result<String> {
    let script = runner_path(root);
    if !script.exists() {
        return Err(anyhow!("找不到启动脚本: {}", script.display()));
    }

    let output = powershell_command()
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script)
        .arg(action)
        .arg("all")
        .current_dir(root)
        .output()
        .with_context(|| format!("执行 {} {} 失败", RUNNER_SCRIPT, action))?;

    command_output(output, action)
}

fn command_output(output: Output, action: &str) -> anyhow::Result<String> {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        if stdout.is_empty() {
            Ok(format!("BeeBotOS {} 已完成。", action))
        } else {
            Ok(stdout)
        }
    } else if stderr.is_empty() {
        Err(anyhow!("BeeBotOS {} 失败。", action))
    } else {
        Err(anyhow!("{}", stderr))
    }
}

fn powershell_command() -> Command {
    let mut command = Command::new("powershell.exe");
    hide_process_window(&mut command);
    command
}

#[cfg(target_os = "windows")]
fn hide_process_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
}

#[cfg(not(target_os = "windows"))]
fn hide_process_window(_command: &mut Command) {}

#[cfg(target_os = "windows")]
fn run_ui(_root: PathBuf) -> anyhow::Result<()> {
    beebotos_launcher::windows::run();
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn run_ui(_root: PathBuf) -> anyhow::Result<()> {
    println!("BeeBotOS Launcher GUI is available on Windows. Use beebotos-run.ps1 on Windows.");
    Ok(())
}

#[cfg(target_os = "windows")]
fn show_info(title: &str, message: &str) {
    let _ = native_windows_gui::init();
    native_windows_gui::simple_message(title, message);
}

#[cfg(not(target_os = "windows"))]
fn show_info(title: &str, message: &str) {
    println!("{title}\n{message}");
}

#[cfg(target_os = "windows")]
fn show_error(title: &str, message: &str) {
    let _ = native_windows_gui::init();
    native_windows_gui::simple_message(title, message);
}

#[cfg(not(target_os = "windows"))]
fn show_error(title: &str, message: &str) {
    eprintln!("{title}: {message}");
}
