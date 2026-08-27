pub mod status;
pub mod utils;
pub(crate) mod win_inspector;
#[cfg(windows)]
pub(crate) mod win_spawn;

use crate::config;
use crate::service::download;
use crate::service::workflow::utils::{is_port_in_use, spawn_output_readers};
use std::collections::HashMap;

#[cfg(windows)]
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use tauri::Manager;

/// 启动守卫：并发调用 `launch` 时只允许一个真正拉起 dsh 进程
static LAUNCH_GUARD: AtomicBool = AtomicBool::new(false);
/// 当前进程内由桌面端创建的 MIR3 AI Core 根进程 PID；0 表示没有持有的实例。
static OWNED_PROCESS_ID: AtomicU32 = AtomicU32::new(0);
/// Windows 进程句柄用于确认 PID 仍指向原进程，消除 PID 复用误杀窗口。
#[cfg(windows)]
static OWNED_PROCESS_HANDLE: AtomicUsize = AtomicUsize::new(0);

struct LaunchGuard;

impl Drop for LaunchGuard {
    fn drop(&mut self) {
        LAUNCH_GUARD.store(false, Ordering::SeqCst);
    }
}

/// 等待并取得启动所有权；并发调用不能在首个启动尚未登记 PID 时提前返回成功。
async fn acquire_launch_guard() -> Result<Option<LaunchGuard>, String> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if has_owned_process() {
            return Ok(None);
        }
        if LAUNCH_GUARD
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Ok(Some(LaunchGuard));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(
                "HARNESS_LAUNCH_LOCK_TIMEOUT: another MIR3 AI Core launch did not settle"
                    .to_string(),
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// 从起始端口向上查找第一个空闲端口，绝不结束未知的端口占用进程。
fn find_available_port(start: u16) -> Result<u16, String> {
    let mut port = start;
    loop {
        if !is_port_in_use(port) {
            return Ok(port);
        }
        log::warn!("Port {port} is occupied, trying the next port");
        port = port.checked_add(1).ok_or_else(|| {
            "PORT_EXHAUSTED: no available TCP port after the configured port".to_string()
        })?;
    }
}

/// dsh 版本是否支持 `--no-open` 标志。
///
/// 0.1.0-rc.8 起 `dsh web` 默认在系统浏览器打开 UI（桌面端内嵌 WebView，
/// 不希望每次启动都弹浏览器），并新增 `--no-open` 关闭该行为。更早的 rc
/// 版本没有这个标志，commander 会把未知选项当作错误、导致 web profile
/// 启动失败，因此追加标志前必须按已装版本判定：0.1.0-rc.8 及以上传标志；
/// 更早不传（保持旧行为）。
///
/// 比较用 `semver` 库按完整语义化版本进行：只比 rc 序号会把基础版本更大的
/// 新版本误判为旧版——`0.1.1-rc.1` 的 rc 号（1）虽小于 8，但晚于
/// 0.1.0-rc.8，同样支持 `--no-open`（该误判是浏览器复弹的回归根因）。
/// 版本号非法（无法解析）时保守处理：不追加标志。
fn version_supports_no_open(version: &str) -> bool {
    // 首个支持 `--no-open` 的 dsh 版本（0.1.0-rc.8）
    const NO_OPEN_MIN_VERSION: &str = "0.1.0-rc.8";
    let Ok(min) = semver::Version::parse(NO_OPEN_MIN_VERSION) else {
        return false;
    };
    semver::Version::parse(version)
        .map(|v| v >= min)
        .unwrap_or(false)
}

/// 按当前活动核心的 dsh 版本决定是否追加 `--no-open`（见 [`version_supports_no_open`]）。
///
/// 版本以活动核心为准：本地核心（用户 CLI 安装）与预打包核心各自读自己的
/// 包清单；读不到时保守处理：不追加标志。
fn web_supports_no_open_flag(app_handle: &tauri::AppHandle) -> bool {
    match crate::service::core::active_version(app_handle) {
        Some(version) => version_supports_no_open(&version),
        None => false,
    }
}

/// 只结束本应用当前进程创建并仍持有的 MIR3 AI Core 进程树。
fn terminate_owned_process() {
    let pid = OWNED_PROCESS_ID.swap(0, Ordering::SeqCst);
    if pid == 0 {
        return;
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::WaitForSingleObject;
        const WAIT_TIMEOUT_CODE: u32 = 0x0000_0102;
        let handle_value = OWNED_PROCESS_HANDLE.swap(0, Ordering::SeqCst);
        if handle_value == 0 {
            return;
        }
        let handle = handle_value as windows_sys::Win32::Foundation::HANDLE;
        // 真实句柄已结束说明 PID 可能已复用，此时绝不调用 taskkill。
        if unsafe { WaitForSingleObject(handle, 0) } != WAIT_TIMEOUT_CODE {
            unsafe { CloseHandle(handle) };
            return;
        }
        kill_pid_tree(pid);
        unsafe {
            WaitForSingleObject(handle, 5_000);
            CloseHandle(handle);
        }
    }

    #[cfg(unix)]
    {
        kill_pid_tree(pid);
    }
}

/// 结束进程树（Windows `taskkill /PID <pid> /T /F`；Unix 负 PID 进程组，与
/// 启动时 `process_group(0)` 对应）。调用方需先确认 PID 确实指向目标进程。
fn kill_pid_tree(pid: u32) {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/T", "/F"]);
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        if let Err(e) = cmd.output() {
            log::error!("Failed to stop MIR3 AI Core process tree {pid}: {e}");
        }
    }

    #[cfg(unix)]
    {
        // MIR3 AI Core 根进程启动在独立进程组中，负 PID 只作用于该进程树。
        let group = format!("-{pid}");
        let _ = Command::new("kill").args(["-TERM", "--", &group]).output();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let _ = Command::new("kill").args(["-KILL", "--", &group]).output();
    }
}

pub fn has_owned_process() -> bool {
    OWNED_PROCESS_ID.load(Ordering::SeqCst) != 0
}

/// 结束所有从本应用 dsh 安装目录启动的 MIR3 AI Core 服务进程（含历史崩溃残留的孤儿实例）。
///
/// 只停本应用当前持有的进程不够：`.mir3-core.pid` 标记只记录最近一次会话的 PID，
/// 应用多次崩溃/强杀（任务管理器结束等）会遗留多个孤儿 dsh 进程、端口一路漂移
/// （3080→3081→…），`sweep_orphan_core` 每次只能回收最近一个，更早的孤儿
/// 会持续占用 `dependencies/dsh` 目录的文件句柄（node 以该目录为 cwd 且模块
/// DLL 加载在内存），更新切换目录时触发 os error 32（INSTALL_BACKUP_FAILED）。
///
/// 命令行为本应用 dsh 入口路径（`...\dependencies\dsh\node_modules\...\bin.js`）
/// 的 node 进程可判定为本应用的服务实例——路径精确匹配不会误杀用户其它 node
/// 程序，因此可安全地全部结束（taskkill /T /F）。
pub fn terminate_stale_core_processes(app_handle: &tauri::AppHandle) {
    // 开发（debug）构建不做按路径清扫：生产与开发共用同一个 `dependencies/dsh`
    // 安装目录（核心共用），按命令行路径匹配会把同时运行的 release 服务进程
    // 一并结束——`pnpm tauri dev` 每次后端重编译都会重启应用并触发清扫，导致
    // "release 版 DSH 被 dev 版热更新杀掉"。开发构建自身的崩溃残留仍由
    // `.mir3-core.pid` 标记（位于独立开发数据目录，PID+端口双重确认）
    // 精确回收。
    if cfg!(debug_assertions) {
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let dsh_bin_path = config::get_dsh_binary_path(app_handle);
        let Some(dsh_bin) = dsh_bin_path.to_str() else {
            return;
        };
        // 进程名过滤保证 PowerShell 自身（其命令行同样包含该路径）不被误杀；
        // 路径中的单引号按 PS 字符串字面量规则转义，避免用户目录含 `'` 时语法错误。
        let escaped = dsh_bin.replace('\'', "''");
        let script = format!(
            "Get-CimInstance Win32_Process -Filter \"Name = 'node.exe'\" | Where-Object {{ $_.CommandLine -like '*{escaped}*' }} | Select-Object -ExpandProperty ProcessId"
        );
        let Ok(output) = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(0x08000000)
            .output()
        else {
            log::error!("Failed to enumerate stale MIR3 AI Core service processes");
            return;
        };
        let mut found = 0;
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let Ok(pid) = line.trim().parse::<u32>() else {
                continue;
            };
            found += 1;
            log::warn!(
                "Terminating stale MIR3 AI Core service process {pid} (from dsh install dir)"
            );
            kill_pid_tree(pid);
        }
        if found > 0 {
            // 与 stop() 同理：taskkill 返回后 DLL 句柄的释放还有短暂滞后，
            // 让出一点时间避免紧随其后的目录切换撞上残留锁。
            std::thread::sleep(std::time::Duration::from_millis(800));
        }
    }
    #[cfg(not(windows))]
    {
        // Unix 允许对打开中的文件重命名，孤儿进程不阻塞更新切换，无需处理。
        let _ = app_handle;
    }
}

// ---------------------------------------------------------------------------
// 孤儿 MIR3 AI Core 清扫：崩溃/强杀残留实例的识别与回收（issue #34 关联现象）
// ---------------------------------------------------------------------------

/// 孤儿清扫用的 PID/端口标记文件路径（$MIR3_STUDIO_HOME/.mir3-core.pid，两行：PID、端口）。
///
/// 应用被强杀（崩溃、任务管理器结束等）时无法执行退出清理，其 MIR3 AI Core 子进程
/// 会继续占用端口；下一次启动只能一路漂移端口（3080→3081→…）并触发服务端
/// "already running"，表现为应用"坏掉"。启动前据此文件识别并清理这类残留。
fn core_pid_path(app_handle: &tauri::AppHandle) -> std::path::PathBuf {
    config::get_dsh_data_path(app_handle).join(".mir3-core.pid")
}

/// 记录本次启动的 MIR3 AI Core PID 与端口，供下次启动清扫孤儿用。
fn persist_core_pid(app_handle: &tauri::AppHandle, pid: u32, port: u16) {
    let path = core_pid_path(app_handle);
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(&path, format!("{pid}\n{port}\n"));
}

/// 启动前清扫上次崩溃残留的孤儿 MIR3 AI Core。端口与 PID 双重确认后才动手：
/// - 标记进程已死 → 仅清理陈旧标记；
/// - 端口占用者正是标记中的 PID → 本应用残留，结束其进程树并清标记；
/// - 其余情况（标记不可解析、端口被其他程序占用、无法探测占用者）一律不动，
///   绝不凭端口猜进程、绝不杀未知进程。
pub fn sweep_orphan_core(app_handle: &tauri::AppHandle) {
    if has_owned_process() {
        return;
    }
    // 先按命令行路径清扫所有从本应用 dsh 安装目录启动的孤儿 MIR3 AI Core 实例：
    // 标记文件只记录最近一次会话的 PID，应用多次崩溃/强杀会遗留更早的孤儿
    // （端口一路漂移 3081/3082/…），它们持续占用 dependencies/dsh 目录的文件
    // 句柄，导致更新切换目录失败（INSTALL_BACKUP_FAILED, os error 32）。
    // 路径精确匹配不会误杀用户其它 node 程序；标记中的进程若在其中会被一并
    // 结束，随后的 PID/端口双重确认自然落空，仅清理陈旧标记。
    terminate_stale_core_processes(app_handle);
    let pid_file = core_pid_path(app_handle);
    let Ok(text) = fs::read_to_string(&pid_file) else {
        return;
    };
    let mut lines = text.lines();
    let (Some(pid), Some(port)) = (
        lines.next().and_then(|l| l.trim().parse::<u32>().ok()),
        lines.next().and_then(|l| l.trim().parse::<u16>().ok()),
    ) else {
        // 标记内容不可解析：陈旧垃圾，清掉即可
        let _ = fs::remove_file(&pid_file);
        return;
    };
    if !is_port_in_use(port) {
        // 端口已释放：残留实例早已自行退出，仅清理标记
        let _ = fs::remove_file(&pid_file);
        return;
    }
    if port_owner_pid(port) != Some(pid) {
        // 端口占用者不是我们落盘的进程（或探测不到）：可能是其他程序，不动
        return;
    }
    log::warn!(
        "Sweeping orphaned MIR3 AI Core process {pid} (port {port}) left by a previous session"
    );
    kill_pid_tree(pid);
    let _ = fs::remove_file(&pid_file);
}

/// 占用指定端口的进程 PID（LISTENING 状态）。
/// - Windows：`netstat -ano` 解析；
/// - Unix：`lsof -ti tcp:<port>`，不可用时返回 None。
///
/// 返回 None 视为"无法确认"，调用方不会因此杀任何进程。
fn port_owner_pid(port: u16) -> Option<u32> {
    #[cfg(windows)]
    {
        let output = Command::new("netstat").arg("-ano").output().ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let needle = format!(":{port} ");
        for line in text.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 5 || fields[0] != "TCP" {
                continue;
            }
            // 本地地址列（如 127.0.0.1:3080 / [::1]:3080）以 :<port> 结尾
            if !fields[1].ends_with(&needle) {
                continue;
            }
            if fields[3] == "LISTENING" {
                return fields[4].parse().ok();
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        // lsof 在 macOS 默认可用、Linux 常缺失；缺失时跳过清扫（返回 None）
        let output = Command::new("lsof")
            .args(["-ti", &format!("tcp:{port}")])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .and_then(|l| l.trim().parse().ok())
    }
}

/// Windows RedirectionGuard（错误码 448 = ERROR_UNTRUSTED_MOUNT_POINT）逃逸重拉的标记路径。
#[cfg(windows)]
fn relaunch_marker_path(app_handle: &tauri::AppHandle) -> std::path::PathBuf {
    config::get_base_dir(app_handle).join(".dsh-relaunch-448")
}

/// 探测 dsh 入口在当前进程上下文下的打开错误码（None=可打开）。
///
/// 448 只在「进程继承 RedirectionGuard 强制执行」时出现；干净上下文（父进程为
/// explorer 等普通进程）下 Level-1 符号链接可正常穿越。
#[cfg(windows)]
fn dsh_bin_open_error(app_handle: &tauri::AppHandle) -> Option<i32> {
    std::fs::File::open(config::get_dsh_binary_path(app_handle))
        .err()
        .and_then(|e| e.raw_os_error())
}

/// 通过 explorer 转交启动请求，脱离 RedirectionGuard 强制执行上下文后退出本进程。
///
/// Windows 11 25H2 的 RedirectionGuard 对「非提权进程创建的符号链接/联接点」盖信任章，
/// 而安装器（msiexec/RestartManager 自动重开）会在自身进程启用强制执行并随进程树继承，
/// 导致新实例跨越 pnpm 符号链接链打开 bin.js 时持续报 448——实测与等待时长无关、
/// 永不自行恢复（issue #35）。应用无法在运行时关闭继承的策略，只能脱离被污染的进程树：
/// 把启动请求转交给 explorer（单实例壳进程，干净上下文），由 explorer 创建新实例，
/// 其父进程即 explorer，不再继承强制执行（实测：explorer.exe <exe> 的子进程父进程为
/// explorer.exe，而非转交发起者）。标记文件用于防死循环：若上次重拉未逃逸
/// （explorer 未运行等），本次回退到常规缺失处理（复位 installed 走安装流程）。
#[cfg(windows)]
fn relaunch_via_shell_escape(app_handle: &tauri::AppHandle) {
    let marker = relaunch_marker_path(app_handle);
    if marker.exists() {
        let _ = std::fs::remove_file(&marker);
        log::warn!("RedirectionGuard(448) relaunch did not escape, falling back to normal missing handling");
        return;
    }
    let _ = std::fs::write(&marker, b"1");
    let Ok(exe) = std::env::current_exe() else {
        log::warn!("RedirectionGuard(448) detected but current_exe unavailable, falling back");
        return;
    };
    match std::process::Command::new("explorer.exe").arg(&exe).spawn() {
        Ok(_) => {
            log::warn!(
                "RedirectionGuard(448) detected, relaunching via explorer to escape enforced context: {}",
                exe.display()
            );
            // 短暂让出后退出，避免与新实例产生单实例冲突
            std::thread::sleep(std::time::Duration::from_millis(300));
            std::process::exit(0);
        }
        Err(e) => {
            log::warn!(
                "RedirectionGuard(448) detected but explorer spawn failed ({e}), falling back"
            );
        }
    }
}

/// 检测并启动 MIR3 AI Core 服务
pub async fn start(app_handle: tauri::AppHandle) -> Result<(), String> {
    let setting = config::get_store_dat_setting(&app_handle);
    let node_binary_path = config::get_node_binary_path(&app_handle);
    // 活动核心的入口：本地核心存在时优先本地（需求 3），否则预打包
    let dsh_binary_path = crate::service::core::active_dsh_binary(&app_handle);

    if !setting.installed {
        log::debug!("MIR3 AI Core not installed, skipping startup");
        return Ok(());
    }
    if !node_binary_path.exists() || !dsh_binary_path.exists() {
        // Windows RedirectionGuard(448)：安装器继承的强制执行上下文永不自行恢复，
        // 先尝试通过 explorer 逃逸重拉（见 relaunch_via_shell_escape 注释），
        // 成功则本进程退出；未命中（重拉未逃逸/非 448）才走常规缺失处理。
        #[cfg(windows)]
        if dsh_bin_open_error(&app_handle) == Some(448) {
            relaunch_via_shell_escape(&app_handle);
        }
        let mut setting = config::get_store_dat_setting(&app_handle);
        setting.installed = false;
        config::set_store_dat_setting(&app_handle, setting);
        // 状态变更需要 info 级落盘：这是「store 显示未安装」的源头之一
        // （核心文件短暂缺失被复位），自更新后自动重开走进安装分支多由此触发。
        log::info!("Runtime files missing (node/dsh), resetting installed flag");
        return Ok(());
    }

    if has_owned_process() {
        log::info!("Owned MIR3 AI Core process is already running");
        status::set_status(status::Status::Running);
        status::emit_status(&app_handle);
        return Ok(());
    }

    // 清理 RedirectionGuard(448) 逃逸重拉标记：本进程正常走到启动说明处于干净上下文，
    // 移除标记保证下次自更新后仍能触发逃逸重拉。
    #[cfg(windows)]
    let _ = std::fs::remove_file(relaunch_marker_path(&app_handle));

    log::info!("Starting MIR3 AI Core service");
    status::set_status(status::Status::Starting);
    status::emit_status(&app_handle);
    launch(app_handle).await?;
    // 之后由 scheduler/task/tick_check_dsh_process/mod.rs 检测状态

    Ok(())
}

/// 重启 MIR3 AI Core 服务
pub async fn restart(app_handle: tauri::AppHandle) -> Result<(), String> {
    log::info!("Restarting MIR3 AI Core service");

    // 1. 停止现有服务
    stop(app_handle.clone()).await?;

    // 2. 重新启动
    start(app_handle).await?;

    Ok(())
}

/// 启动 MIR3 AI Core 服务进程
pub async fn launch(app_handle: tauri::AppHandle) -> Result<(), String> {
    let mut setting = config::get_store_dat_setting(&app_handle);
    let node_binary_path = config::get_node_binary_path(&app_handle);
    // 活动核心的 dsh 入口（本地核心优先，未检测到走预打包）
    let dsh_binary_path = crate::service::core::active_dsh_binary(&app_handle);

    log::debug!("Checking Node.js path: {:?}", node_binary_path);
    if !node_binary_path.exists() {
        log::error!("Node.js not installed");
        return Err("NODE_NOT_FOUND: Node.js not installed".to_string());
    }
    log::debug!("Checking MIR3 AI Core path: {:?}", dsh_binary_path);
    if !dsh_binary_path.exists() {
        log::error!("MIR3 AI Core not installed");
        return Err("HARNESS_NOT_FOUND: MIR3 AI Core not installed".to_string());
    }

    // setup 自动启动与前端 boot 可能并发进入。后到者必须等待首个调用登记 PID，
    // 不能提前返回成功，否则紧随其后的健康检查会误报 HARNESS_NOT_OWNED。
    let Some(_launch_guard) = acquire_launch_guard().await? else {
        log::info!("Owned MIR3 AI Core process is already running, skipping launch");
        return Ok(());
    };

    // 端口冲突时从当前值开始逐个递增，并持久化最终选择供所有调用方复用。
    let available_port = find_available_port(setting.port)?;
    if available_port != setting.port {
        log::info!(
            "MIR3 AI Core port changed from {} to {} because the configured port is occupied",
            setting.port,
            available_port
        );
        setting.port = available_port;
        config::set_store_dat_setting(&app_handle, setting.clone());
    }

    // 构造环境变量：隔离的 $MIR3_STUDIO_HOME + 隐私默认（关闭遥测）
    let dsh_home = config::get_dsh_data_path(&app_handle);
    fs::create_dir_all(&dsh_home).map_err(|e| format!("create dsh home failed: {e}"))?;

    // Windows 极简模式修复的自愈：插件已装入 profile 时确保 patch 挂载行与
    // minimal-win 用户 preset 落盘（幂等）。最佳努力：失败只告警，不阻断启动。
    if let Err(e) = win_inspector::apply(&app_handle) {
        log::warn!("win32 terminal support apply failed: {e}");
    }
    // 预防性处理：pnpm 在无 TTY 环境（dsh-market 等子进程）下重装/更新插件时，
    // 清理/重建 node_modules 会触发交互确认并因无 TTY 直接中止
    // （ERR_PNPM_ABORTED_REMOVE_MODULES_DIR_NO_TTY），表现为插件更新失败。
    // 启动时确保 profile 的 .npmrc 写入 confirmModulesPurge=false（幂等、保留
    // 已有配置）。最佳努力：失败只告警，不阻断启动。
    if let Err(e) = crate::service::plugin::ensure_profile_npmrc(&app_handle) {
        log::warn!("ensure profile .npmrc failed: {e}");
    }
    // 第一方 MIR3 插件、Skill 与项目绑定 MCP 为产品能力，启动前幂等安装；
    // 不进入可跳过的社区预装流程。
    crate::service::plugin::system::ensure(&app_handle)?;
    let mut envs: HashMap<String, String> = HashMap::new();
    envs.insert(
        config::core_compat::CORE_HOME_ENV.to_string(),
        dsh_home.to_string_lossy().into_owned(),
    );
    envs.insert("DSH_TELEMETRY_DISABLED".to_string(), "1".to_string());
    envs.insert("NO_COLOR".to_string(), "1".to_string());
    envs.insert("DSH_WEB_PORT".to_string(), setting.port.to_string());
    envs.insert(
        "MIR3_STUDIO_HOME".to_string(),
        dsh_home.to_string_lossy().into_owned(),
    );
    if let Some(project) = app_handle
        .state::<crate::service::project::ProjectService>()
        .store()
        .active_project()?
    {
        envs.insert("MIR3_ACTIVE_PROJECT_ID".to_string(), project.id);
        envs.insert("MIR3_ACTIVE_PROJECT_ROOT".to_string(), project.root);
        envs.insert(
            "MIR3_ACTIVE_WORKSPACE_ROOT".to_string(),
            project.active_workspace_root,
        );
    }
    if let Some(path) = crate::service::project::mcp_binary_path(&app_handle) {
        envs.insert(
            "MIR3_MCP_BIN".to_string(),
            path.to_string_lossy().into_owned(),
        );
    }

    // 扩展 PATH，让 dsh 及其子进程能找到 node；Windows 上再注入 Git Bash 的
    // bin 目录：persistent bash（--noprofile --norc）不执行 profile 脚本、PATH
    // 完全继承服务进程，若不含 Git 的 usr/bin，ls/sed/find 等 coreutils 全会
    // `command not found`（MSYS 运行时在部分环境下不会自动补 /usr/bin）。
    if let Some(node_dir) = node_binary_path.parent() {
        if let Some(existing_path) = std::env::var_os("PATH") {
            let git_dirs = win_inspector::git_bash_bin_dirs();
            // 只打印注入的前缀目录，完整 PATH 太长会刷屏
            for dir in &git_dirs {
                log::debug!("harness service PATH prepend: {}", dir.to_string_lossy());
            }
            let mut paths = vec![node_dir.to_path_buf()];
            paths.extend(git_dirs);
            paths.extend(std::env::split_paths(&existing_path));
            if let Ok(new_path) = std::env::join_paths(paths) {
                envs.insert("PATH".to_string(), new_path.to_string_lossy().into_owned());
            }
        }
    }

    // 日志文件（前端日志面板读取）。
    // 每次真实启动前轮转：只保留最近 3 次启动的日志，旧文件后退为
    // `dsh-web.log.1` / `dsh-web.log.2`，避免单文件随多次启动无限增长。
    let log_path = config::get_service_log_path(&app_handle);
    fs::create_dir_all(log_path.parent().unwrap_or(std::path::Path::new(".")))
        .map_err(|e| format!("create log dir failed: {e}"))?;
    utils::rotate_service_log(&log_path, 3);

    // rc.8 起 `dsh web` 默认在系统浏览器打开 UI；桌面端内嵌 WebView，不需要
    // 浏览器，追加 `--no-open` 关闭（老版本无此标志时按版本判定不传）。
    let no_open = web_supports_no_open_flag(&app_handle);

    log::info!("Starting MIR3 AI Core process");

    // Windows 打包版是 GUI 进程（没有控制台）。直接以 CREATE_NO_WINDOW 启动
    // node 会让 dsh 派生的子进程各自新建可见控制台窗口（频繁闪烁 cmd 黑窗），
    // 因此 Windows 上改用“隐藏控制台”方式启动，见 win_spawn 模块。
    let active_profile = crate::service::profile::active_profile(&app_handle);
    let spawn_result = {
        #[cfg(windows)]
        {
            let mut args: Vec<OsString> = vec![
                dsh_binary_path.as_os_str().to_os_string(),
                OsString::from("--profile"),
                OsString::from(active_profile.as_str()),
                OsString::from("--host"),
                OsString::from("127.0.0.1"),
                OsString::from("--port"),
                OsString::from(setting.port.to_string()),
            ];
            if no_open {
                args.push(OsString::from("--no-open"));
            }
            win_spawn::spawn_with_hidden_console_owned(
                &node_binary_path,
                &args,
                Some(&config::get_dsh_install_path(&app_handle)),
                &envs,
            )
            .map(|(stdout, stderr, pid, handle)| {
                OWNED_PROCESS_ID.store(pid, Ordering::SeqCst);
                // 持有真实进程句柄直到退出；退出后仅在 PID 仍匹配时清空，避免复用。
                let handle_value = handle as usize;
                OWNED_PROCESS_HANDLE.store(handle_value, Ordering::SeqCst);
                std::thread::spawn(move || unsafe {
                    use windows_sys::Win32::Foundation::CloseHandle;
                    use windows_sys::Win32::System::Threading::{
                        GetExitCodeProcess, WaitForSingleObject, INFINITE,
                    };
                    let process_handle = handle_value as windows_sys::Win32::Foundation::HANDLE;
                    WaitForSingleObject(process_handle, INFINITE);
                    // 记录退出码：启动即崩溃（插件冲突等）时前端据此快速失败，
                    // 退出码也便于诊断问题
                    let mut exit_code: u32 = 0;
                    if GetExitCodeProcess(process_handle, &mut exit_code) != 0 {
                        log::warn!("Owned MIR3 AI Core process {pid} exited with code {exit_code}");
                    } else {
                        log::warn!(
                            "Owned MIR3 AI Core process {pid} exited (exit code unavailable)"
                        );
                    }
                    let _ = OWNED_PROCESS_ID.compare_exchange(
                        pid,
                        0,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    );
                    let owns_handle = OWNED_PROCESS_HANDLE
                        .compare_exchange(handle_value, 0, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok();
                    if owns_handle {
                        CloseHandle(process_handle);
                    }
                });
                (Some(stdout), Some(stderr), pid)
            })
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::process::CommandExt;
            let mut cmd = Command::new(&node_binary_path);
            cmd.arg(&dsh_binary_path)
                .arg("--profile")
                .arg(active_profile.as_str())
                .arg("--host")
                .arg("127.0.0.1")
                .arg("--port")
                .arg(setting.port.to_string());
            if no_open {
                cmd.arg("--no-open");
            }
            cmd.envs(&envs)
                .current_dir(config::get_dsh_install_path(&app_handle))
                // 核心修正：提供一个空的 stdin 防止 setRawMode 报错
                .stdin(Stdio::null())
                // 使用管道捕获输出，以便在子线程中读取
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                // 独立进程组让停止操作只影响 MIR3 AI Core 及其后代。
                .process_group(0);
            cmd.spawn().map(|mut child| {
                let pid = child.id();
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();
                OWNED_PROCESS_ID.store(pid, Ordering::SeqCst);
                std::thread::spawn(move || {
                    let code = child.wait().ok().and_then(|status| status.code());
                    // 记录退出码：启动即崩溃（插件冲突等）时前端据此快速失败
                    if let Some(code) = code {
                        log::warn!("Owned MIR3 AI Core process {pid} exited with code {code}");
                    } else {
                        log::warn!("Owned MIR3 AI Core process {pid} exited (no exit code)");
                    }
                    let _ = OWNED_PROCESS_ID.compare_exchange(
                        pid,
                        0,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    );
                });
                (stdout, stderr, pid)
            })
        }
    };

    match spawn_result {
        Ok((stdout, stderr, pid)) => {
            log::info!(
                "MIR3 AI Core process started successfully: pid={pid}, port={}",
                setting.port
            );
            // 记录 PID+端口供下次启动清扫崩溃残留的孤儿实例（见 sweep_orphan_core）
            persist_core_pid(&app_handle, pid, setting.port);
            spawn_output_readers(stdout, stderr, log_path);
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to start process: {}", e);
            Err(format!("PROCESS_START_FAILED: {e}"))
        }
    }
}

/// 停止 MIR3 AI Core 服务
pub async fn stop(app_handle: tauri::AppHandle) -> Result<(), String> {
    log::info!("Stopping MIR3 AI Core service...");
    // 重置启动守卫，确保后续 launch 可以重新拉起；仅结束持有的根进程树。
    LAUNCH_GUARD.store(false, Ordering::SeqCst);
    terminate_owned_process();
    // 清理孤儿清扫标记：正常停止的实例不应被下次启动当作残留
    let _ = fs::remove_file(core_pid_path(&app_handle));

    // 给系统一点时间释放端口 (重要！)
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    status::set_status(status::Status::Stopped);
    status::emit_status(&app_handle);
    Ok(())
}

/// 应用退出时同步回收 MIR3 AI Core 进程。
///
/// 退出路径上不更新状态、不做异步等待，只结束当前应用持有的 MIR3 AI Core 进程树。
pub fn stop_on_exit(app_handle: tauri::AppHandle, _port: u16) {
    terminate_owned_process();
    // 正常退出路径同样清理清扫标记（崩溃路径才需要下次启动清扫）
    let _ = fs::remove_file(core_pid_path(&app_handle));
}

/// 安装环境（Node.js 运行时 + 打包的 MIR3 AI Core 发行版）。
///
/// 返回是否真正落盘更新了 MIR3 AI Core（dsh 任务实际下载并解压）；仅重装
/// Node/pnpm 或全部任务被跳过时返回 false，供调用方决定是否重启页面。
pub async fn install(
    app_handle: &tauri::AppHandle,
    mut dsh_latest: Option<download::LatestDshPkg>,
) -> Result<bool, String> {
    log::info!("Starting installation process");
    // dsh 任务（index==1）实际下载解压时置 true
    let mut dsh_updated = false;

    // 安装前先停止本应用持有的 MIR3 AI Core 服务：运行中的 node 进程会把
    // 原生模块 DLL（如 sharp 的 libvips-42.dll）加载进内存并锁住文件，
    // 不停止的话覆盖解压必然失败（Windows os error 32）。
    // 进程归属以启动时记录的 PID 为准，不根据端口结束未知程序。
    if has_owned_process() {
        log::info!("Stopping running MIR3 AI Core service before installation");
        stop(app_handle.clone()).await?;
    }
    // 只停本应用持有的进程还不够：历史崩溃/强杀残留的孤儿 MIR3 AI Core 实例
    // （不在 .mir3-core.pid 标记中）同样从 dependencies/dsh 启动、占用目录文件
    // 句柄，会导致更新切换目录失败（INSTALL_BACKUP_FAILED, os error 32）。
    // 按命令行路径精确清扫所有本应用 dsh 安装目录启动的进程。
    terminate_stale_core_processes(app_handle);

    let window = app_handle
        .get_webview_window("main")
        .ok_or("Failed to get main window")?;
    log::debug!("Main window obtained");
    // 3 个任务 × 下载/解压 2 个阶段
    let mut tracker = download::ProgressTracker::new(&window, 6);
    let bundled_baseline = download::BaselineBundle::load(app_handle)?;
    let tasks: Vec<Box<dyn download::Installable>> = vec![
        Box::new(download::Nodejs),
        Box::new(download::Dsh),
        Box::new(download::Pnpm),
    ];
    log::info!("Task list created, {} tasks total", tasks.len());

    for (index, task) in tasks.iter().enumerate() {
        log::debug!("Processing task {}/{}", index + 1, tasks.len());
        // 已安装但版本/commit 与最新 release 不一致时强制重新下载。
        // 版本优先（与 resolve_update 的判定完全一致）：dsh 的 rc 发布会复用
        // 同一 git commit（record_commit 不变），只比 commit 会把 rc.8 之于
        // rc.7 误判为"已最新"而跳过下载——日志表现为"All installation tasks
        // completed"但实际什么都没下载，重启后仍是旧版，且前端丢掉更新提示。
        let outdated = index == 1
            && dsh_latest.as_ref().is_some_and(|info| {
                let installed_version = config::get_dsh_version(app_handle);
                let latest_version = download::parse_version_from_tag(&info.tag);
                // 版本号可解析且不同 → 必须更新；版本不可解析时退回 commit 比对
                let version_differs =
                    match (installed_version.as_deref(), latest_version.as_deref()) {
                        (Some(a), Some(b)) => a != b,
                        _ => false,
                    };
                version_differs
                    || config::get_dsh_pkg_commit(app_handle).as_deref()
                        != Some(info.commit.as_str())
            });
        let installed = task.check_installed(app_handle);
        if installed && !outdated {
            log::debug!(
                "Task {} already installed and up to date, skipping",
                index + 1
            );
            tracker.skip_phases(2);
            continue;
        }

        log::info!("Task {} not installed, starting installation", index + 1);

        // 1. 下载
        tracker.start_phase(
            "download",
            &format!(
                "{} {}",
                config::i18n::t("install.downloading"),
                task.title()
            ),
        );
        let component = match index {
            0 => download::BaselineComponent::Node,
            1 => download::BaselineComponent::Core,
            2 => download::BaselineComponent::Pnpm,
            _ => return Err("INSTALL_TASK_INVALID: unknown install task".to_string()),
        };
        // 基线只用于补齐缺失组件，不会把用户已安装或已更新的 Core 降级。
        let bundled = if installed {
            None
        } else {
            bundled_baseline
                .as_ref()
                .map(|bundle| bundle.read(component))
                .transpose()?
        };
        let (name, buffer, used_baseline) = if let Some(payload) = bundled {
            tracker.update(
                100.0,
                format!("正在安装内置 {} 基线", task.title()),
                format!("Use bundled baseline archive: {}", payload.archive),
            );
            log::info!(
                "Using installer-embedded baseline for task {}: {}",
                index + 1,
                payload.archive
            );
            (payload.archive, payload.bytes, true)
        } else {
            // 没有内置基线的旧包、修复安装和显式 Core 更新保留联网路径。
            let (urls, name) = if index == 1 {
                let urls = config::get_dsh_download_urls()?;
                let name = urls
                    .first()
                    .and_then(|u| u.rsplit('/').next())
                    .unwrap_or("")
                    .to_string();
                (urls, name)
            } else {
                let url = task.get_download_url()?;
                let name = url.rsplit('/').next().unwrap_or("").to_string();
                (vec![url], name)
            };
            log::debug!("Download URL: {}", urls.join(" -> "));
            log::debug!("File name: {}", name);
            let buffer = download::download_file_from_sources(&tracker, urls).await?;
            log::info!("Download completed, file size: {} bytes", buffer.len());
            let expected_digest = match index {
                0 => download::fetch_node_sha256(task.get_download_url()?.as_str()).await?,
                1 => {
                    if dsh_latest.is_none() {
                        for attempt in 0..3 {
                            match download::fetch_latest_dsh_pkg_info().await {
                                Ok(info) => {
                                    dsh_latest = Some(info);
                                    break;
                                }
                                Err(e) if attempt < 2 => {
                                    log::warn!(
                                        "Retrying dsh release metadata fetch ({}/3), will retry: {}",
                                        attempt + 1,
                                        e
                                    );
                                    tokio::time::sleep(std::time::Duration::from_millis(
                                        500 * (attempt as u64 + 1),
                                    ))
                                    .await;
                                }
                                Err(e) => {
                                    return Err(format!(
                                        "DSH_INTEGRITY_UNAVAILABLE: 无法获取 MIR3 AI Core 发行版的完整性校验信息（{}），请检查网络后重试",
                                        e
                                    ));
                                }
                            }
                        }
                    }
                    dsh_latest
                        .as_ref()
                        .and_then(|info| info.digest.clone())
                        .ok_or_else(|| {
                            "DSH_INTEGRITY_UNAVAILABLE: trusted release digest is required"
                                .to_string()
                        })?
                }
                2 => config::PNPM_SHA256.to_string(),
                _ => unreachable!(),
            };
            download::verify_sha256(&buffer, &expected_digest)?;
            log::info!("Download integrity verified for task {}", index + 1);
            (name, buffer, false)
        };
        tracker.end_phase();

        // 2. 解压
        tracker.start_phase(
            "extract",
            &format!("{} {}", config::i18n::t("install.extracting"), task.title()),
        );
        let dest = task.get_install_path(app_handle);
        log::debug!("Installation path: {:?}", dest);
        download::ensure_extract_with_backup_policy(
            &tracker,
            name,
            buffer,
            dest,
            index == 1 && installed,
        )
        .await?;
        log::info!("Extraction completed");
        tracker.end_phase();

        // 记录本次安装对应的 release tag 与 commit，供下次启动比对
        if index == 1 {
            dsh_updated = true;
            if used_baseline {
                if let Some(bundle) = bundled_baseline.as_ref() {
                    bundle.record_core_install(app_handle);
                }
            } else if let Some(info) = &dsh_latest {
                config::set_dsh_pkg_commit(app_handle, info.commit.clone());
                config::set_dsh_pkg_tag(app_handle, info.tag.clone());
            }
        }
    }

    log::info!("All installation tasks completed");
    tracker.update(
        100.0,
        config::i18n::t("install.done"),
        "All tasks completed".into(),
    );

    Ok(dsh_updated)
}

fn core_update_backup_path(app_handle: &tauri::AppHandle) -> std::path::PathBuf {
    let active = config::get_dsh_install_path(app_handle);
    active
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(format!(".{}.backup", config::DSH_CORE_DIR))
}

/// Studio 已完成 Bridge、普通/归档 Session、MCP 与领域能力 canary 后提交更新。
pub async fn finalize_core_update(app_handle: &tauri::AppHandle) -> Result<bool, String> {
    let backup = core_update_backup_path(app_handle);
    let had_backup = backup.exists();
    if had_backup && !download::remove_dir_with_retry(&backup).await {
        return Err(format!(
            "CORE_UPDATE_BACKUP_CLEAN_FAILED: {}",
            backup.display()
        ));
    }
    let mut setting = config::get_store_dat_setting(app_handle);
    setting.last_known_good_core_tag = setting.dsh_pkg_tag.clone();
    setting.last_known_good_core_commit = setting.dsh_pkg_commit.clone();
    config::set_store_dat_setting(app_handle, setting.clone());
    if let Err(error) = persist_core_canary_state(app_handle, &setting) {
        // canary 本身已经通过且 LKG 已持久化；证据文件失败不能把已删除 backup
        // 的正常 Core 误判为需要回滚，但原生 smoke 会因缺少该文件而失败。
        log::error!("Core canary evidence persistence failed: {error}");
    }
    Ok(had_backup)
}

fn persist_core_canary_state(
    app_handle: &tauri::AppHandle,
    setting: &config::Setting,
) -> Result<(), String> {
    let root = config::get_dsh_data_path(app_handle);
    fs::create_dir_all(&root)
        .map_err(|error| format!("CORE_CANARY_STATE_ROOT_CREATE_FAILED: {error}"))?;
    let path = root.join(".mir3-core-canary.json");
    let temporary = root.join(format!(".mir3-core-canary.{}.tmp", std::process::id()));
    let passed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("CORE_CANARY_STATE_CLOCK_FAILED: {error}"))?
        .as_millis()
        .min(i64::MAX as u128) as i64;
    let value = core_canary_state_value(
        setting,
        &app_handle.package_info().version.to_string(),
        passed_at,
    );
    let bytes = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("CORE_CANARY_STATE_SERIALIZE_FAILED: {error}"))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("CORE_CANARY_STATE_OPEN_FAILED: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("CORE_CANARY_STATE_WRITE_FAILED: {error}"))?;
    drop(file);
    if path.exists() {
        fs::remove_file(&path)
            .map_err(|error| format!("CORE_CANARY_STATE_REPLACE_FAILED: {error}"))?;
    }
    fs::rename(&temporary, &path)
        .map_err(|error| format!("CORE_CANARY_STATE_COMMIT_FAILED: {error}"))?;
    Ok(())
}

fn core_canary_state_value(
    setting: &config::Setting,
    app_version: &str,
    passed_at: i64,
) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "status": "passed",
        "protocolVersion": 2,
        "appVersion": app_version,
        "coreTag": setting.dsh_pkg_tag,
        "coreCommit": setting.dsh_pkg_commit,
        "passedAt": passed_at,
        "checks": [
            "bridge-v2",
            "ordinary-session",
            "archived-system-session",
            "mcp-sidecar",
            "domain-capability"
        ]
    })
}

/// 新 Core 在插件 ready 前失败时恢复更新前目录与最后已知可用版本记录。
pub async fn rollback_core_update(app_handle: &tauri::AppHandle) -> Result<bool, String> {
    let backup = core_update_backup_path(app_handle);
    if !backup.is_dir() {
        return Ok(false);
    }
    if has_owned_process() {
        stop(app_handle.clone()).await?;
    }
    let active = config::get_dsh_install_path(app_handle);
    let failed = active
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(format!(
            ".{}.failed-{}",
            config::DSH_CORE_DIR,
            std::process::id()
        ));
    if failed.exists() && !download::remove_dir_with_retry(&failed).await {
        return Err(format!("CORE_ROLLBACK_CLEAN_FAILED: {}", failed.display()));
    }
    if active.exists() {
        download::rename_with_retry(&active, &failed)
            .await
            .map_err(|e| format!("CORE_ROLLBACK_QUARANTINE_FAILED: {e}"))?;
    }
    if let Err(e) = download::rename_with_retry(&backup, &active).await {
        if failed.exists() {
            let _ = download::rename_with_retry(&failed, &active).await;
        }
        return Err(format!("CORE_ROLLBACK_RESTORE_FAILED: {e}"));
    }
    if failed.exists() && !download::remove_dir_with_retry(&failed).await {
        log::warn!(
            "Failed to remove rejected Core candidate: {}",
            failed.display()
        );
    }
    let mut setting = config::get_store_dat_setting(app_handle);
    setting.dsh_pkg_tag = setting.last_known_good_core_tag.clone();
    setting.dsh_pkg_commit = setting.last_known_good_core_commit.clone();
    setting.active_core = Some("app".to_string());
    config::set_store_dat_setting(app_handle, setting);
    Ok(true)
}

/// 健康检查（通过 Rust 代理，避免 WebView CORS 问题）
pub async fn proxy_health_check(port: u16) -> Result<String, String> {
    if !has_owned_process() {
        return Err("HARNESS_NOT_OWNED: no MIR3 AI Core process is owned by this app".to_string());
    }
    let client = reqwest::Client::builder()
        .timeout(config::HEALTH_CHECK_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;

    for endpoint in [
        format!("http://127.0.0.1:{port}/"),
        format!("http://127.0.0.1:{port}/healthz"),
    ] {
        match client.get(&endpoint).send().await {
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if status.is_success() {
                    return Ok(format!(
                        "healthy - {status} - {}",
                        body.chars().take(80).collect::<String>()
                    ));
                }
            }
            Err(err) => {
                log::debug!("Health check {endpoint}: {err}");
            }
        }
    }
    Err("HARNESS_NOT_READY: MIR3 AI Core service is not ready".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn occupied_port_advances_to_a_free_port() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind occupied test port");
        let occupied = listener.local_addr().expect("read occupied port").port();
        let selected = find_available_port(occupied).expect("find next free port");
        assert!(selected > occupied);
        assert!(!is_port_in_use(selected));
    }

    #[test]
    fn core_canary_evidence_names_every_public_runtime_gate() {
        let setting = config::Setting {
            dsh_pkg_tag: Some("dsh-test".to_string()),
            dsh_pkg_commit: Some("commit-test".to_string()),
            ..Default::default()
        };
        let value = core_canary_state_value(&setting, "test-version", 1234);
        assert_eq!(value["status"], "passed");
        assert_eq!(value["protocolVersion"], 2);
        assert_eq!(value["coreTag"], "dsh-test");
        assert_eq!(value["coreCommit"], "commit-test");
        assert_eq!(value["passedAt"], 1234);
        assert_eq!(value["checks"].as_array().unwrap().len(), 5);
        assert!(value["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "archived-system-session"));
        assert!(value["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "domain-capability"));
    }

    #[tokio::test]
    async fn concurrent_launch_waits_for_the_first_owner_instead_of_returning_early() {
        OWNED_PROCESS_ID.store(0, Ordering::SeqCst);
        LAUNCH_GUARD.store(false, Ordering::SeqCst);
        let first = acquire_launch_guard()
            .await
            .expect("acquire first launch guard")
            .expect("first caller owns launch");
        let waiting = tokio::spawn(acquire_launch_guard());
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!waiting.is_finished());
        drop(first);
        let second = tokio::time::timeout(std::time::Duration::from_secs(1), waiting)
            .await
            .expect("second caller resumes")
            .expect("join second caller")
            .expect("second caller acquires guard")
            .expect("second caller owns launch after first settles");
        drop(second);
        assert!(!LAUNCH_GUARD.load(Ordering::SeqCst));
    }

    #[test]
    fn no_open_supported_on_rc8_and_later() {
        assert!(version_supports_no_open("0.1.0-rc.8"));
        assert!(version_supports_no_open("0.1.0-rc.9"));
        // 基础版本更大的新版本：0.1.1-rc.1 的 rc 号（1）虽小于 8，但晚于
        // 0.1.0-rc.8，同样支持 --no-open（只比 rc 号会把这里误判为旧版）
        assert!(version_supports_no_open("0.1.1-rc.1"));
        assert!(version_supports_no_open("0.1.2-rc.1"));
        // 稳定版必然晚于 rc.8
        assert!(version_supports_no_open("0.1.0"));
        assert!(version_supports_no_open("0.2.0"));
        assert!(version_supports_no_open("1.0.0"));
    }

    #[test]
    fn no_open_absent_before_rc8() {
        assert!(!version_supports_no_open("0.1.0-rc.7"));
        assert!(!version_supports_no_open("0.1.0-rc.0"));
        // 基础版本更早的 rc 系列一律不支持
        assert!(!version_supports_no_open("0.0.1-rc.5"));
        assert!(!version_supports_no_open("0.0.9-rc.99"));
    }

    #[test]
    fn no_open_unknown_version_is_conservative() {
        assert!(!version_supports_no_open(""));
        // rc 号缺失：`0.1.0-rc` 的预发布 [rc] 短于 [rc, 8]，判为早于 rc.8
        assert!(!version_supports_no_open("0.1.0-rc"));
        // 不完整/非法版本号（缺 patch、带 v 前缀、无 semver 结构）：无法解析
        assert!(!version_supports_no_open("0.1"));
        assert!(!version_supports_no_open("v0.1.0"));
        assert!(!version_supports_no_open("not-a-version"));
    }
}
