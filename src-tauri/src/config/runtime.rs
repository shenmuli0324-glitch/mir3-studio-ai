use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

use super::constants::*;
use super::format::get_dsh_service_url;
use super::utils::search_node_binary;
use super::{detect_region, Region};

/// 获取 App Data 基础目录
pub fn get_base_dir<R: Runtime>(app_handle: &AppHandle<R>) -> PathBuf {
    app_handle
        .path()
        .app_data_dir()
        .expect("Failed to resolve app data directory")
}

/// Node.js 官方/镜像下载前缀：国内走 npmmirror，其他直连 nodejs.org
fn node_base_url(region: Region) -> &'static str {
    match region {
        Region::Domestic => NODE_MIRROR_BASE_URL,
        Region::Overseas => NODE_BASE_URL,
    }
}

/// Node.js 运行时下载地址
pub fn get_node_download_url() -> Result<String, String> {
    let arch = env::consts::ARCH;
    let os = env::consts::OS;

    // 抽象文件名逻辑
    let filename = match (os, arch) {
        ("macos", "aarch64") => format!("node-{}-darwin-arm64.tar.gz", NODE_VERSION),
        ("macos", "x86_64") => format!("node-{}-darwin-x64.tar.gz", NODE_VERSION),
        ("windows", _) => format!("node-{}-win-x64.zip", NODE_VERSION),
        _ => return Err(format!("Unsupported platform: {} {}", os, arch)),
    };

    Ok(format!(
        "{}/{}/{}",
        node_base_url(detect_region()),
        NODE_VERSION,
        filename
    ))
}

/// 打包的 MIR3 AI Core 兼容发行版下载前缀：恒为 GitHub Release 官方直连，
/// 作为首选下载源（镜像 ghfast.top 中转不稳定，仅作官方失败后的兜底）。
fn dsh_core_base_url() -> &'static str {
    super::core_compat::CORE_RELEASE_BASE
}

/// 打包的 MIR3 AI Core 兼容发行版镜像下载前缀。
fn dsh_mirror_base_url() -> &'static str {
    super::core_compat::CORE_RELEASE_MIRROR_BASE
}

/// MIR3 AI Core 发行版资产文件名（按平台与架构）
fn dsh_pkg_asset_filename() -> Result<String, String> {
    let arch = env::consts::ARCH;
    let os = env::consts::OS;

    super::core_compat::asset_filename(os, arch).map(str::to_string)
}

/// 打包的 MIR3 AI Core 兼容发行版下载地址。
pub fn get_dsh_download_url() -> Result<String, String> {
    Ok(format!(
        "{}{}",
        dsh_core_base_url(),
        dsh_pkg_asset_filename()?
    ))
}

/// 打包的 MIR3 AI Core 兼容发行版下载地址列表（按顺序依次尝试）：
/// GitHub 官方直连 → ghfast.top 镜像兜底。官方直连失败时由下载层自动
/// 切换镜像并告知用户，避免 ghfast.top 不稳定导致首次安装失败。
pub fn get_dsh_download_urls() -> Result<Vec<String>, String> {
    let filename = dsh_pkg_asset_filename()?;
    Ok(vec![
        format!("{}{}", dsh_core_base_url(), filename),
        format!("{}{}", dsh_mirror_base_url(), filename),
    ])
}

/// 为任意 GitHub Release 资产 URL 生成 ghfast.top 镜像兜底地址
/// （透传原 URL，下载内容一致，仍可做 SHA-256 完整性校验）。
pub fn mirror_download_url(asset_url: &str) -> String {
    format!("{DSH_MIRROR_PREFIX}{asset_url}")
}

/// 指定 tag 的 MIR3 AI Core 兼容发行版下载地址。
///
/// 把 latest 下载地址中的 `releases/latest/download/` 替换为
/// `releases/download/<tag>/`，镜像/直连与平台文件名逻辑与最新版完全一致
/// （GitHub 的 tag 下载路径是固定的 release 资产地址，可被确定性推导）。
pub fn get_dsh_download_url_for_tag(tag: &str) -> Result<String, String> {
    let base = dsh_core_base_url().replace(
        "releases/latest/download/",
        &format!("releases/download/{tag}/"),
    );
    Ok(format!("{}{}", base, dsh_pkg_asset_filename()?))
}

/// 在 PATH 及常见安装目录中查找 node 可执行文件（不校验版本）
fn find_local_node_binary() -> Option<PathBuf> {
    let bin_name = if cfg!(windows) { "node.exe" } else { "node" };

    let path_dirs: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .filter(|dir| !dir.as_os_str().is_empty())
            .collect();

    // macOS 上从 Finder/launchd 启动时 PATH 可能不完整，补充常见安装目录
    #[cfg(target_os = "macos")]
    let dirs: Vec<PathBuf> = {
        let mut dirs = path_dirs;
        dirs.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]);
        dirs
    };

    #[cfg(not(target_os = "macos"))]
    let dirs = path_dirs;

    for dir in dirs {
        let candidate = dir.join(bin_name);
        if candidate.is_file() && is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|meta| meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true
}

/// 运行 `node --version` 并捕获输出
///
/// Windows 打包版是 GUI 进程（没有控制台），必须以 CREATE_NO_WINDOW 启动
/// node.exe，否则每次版本检查都会闪现一个黑色 cmd 窗口。
fn node_version_output(node: &Path) -> Option<std::process::Output> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new(node)
            .arg("--version")
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output()
            .ok()
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(node)
            .arg("--version")
            .output()
            .ok()
    }
}

/// 获取指定 Node.js 二进制的版本号（例如 "22.22.0"）
fn get_node_version_of(node: &Path) -> Option<String> {
    let output = node_version_output(node)?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout.trim().trim_start_matches('v');
    if version.is_empty() {
        None
    } else {
        Some(version.to_string())
    }
}

/// 检测本地是否存在版本兼容的 Node.js 环境，返回其二进制路径
pub fn get_local_node_path() -> Option<PathBuf> {
    let node = find_local_node_binary()?;
    let version = get_node_version_of(&node)?;
    is_supported_node_version(&version).then_some(node)
}

/// Node.js 二进制路径
///
/// 优先级：本地版本兼容的 Node.js 环境 > 已安装的捆绑运行时
pub fn get_node_binary_path(app_handle: &tauri::AppHandle) -> PathBuf {
    if let Some(local_node) = get_local_node_path() {
        log::debug!("Using local Node.js: {}", local_node.display());
        return local_node;
    }

    let runtime_dir = get_node_install_path(app_handle);
    // 使用 cfg 宏在编译时确定文件名
    let (rel_path, bin_name) = if cfg!(windows) {
        ("", "node.exe")
    } else {
        ("bin", "node")
    };
    let direct_path = runtime_dir.join(rel_path).join(bin_name);
    if direct_path.exists() {
        direct_path
    } else {
        // 只有在直接路径不存在时才进行开销较大的递归搜索
        search_node_binary(&runtime_dir, bin_name).unwrap_or(direct_path)
    }
}

pub fn get_node_install_path(app_handle: &tauri::AppHandle) -> PathBuf {
    get_base_dir(app_handle).join("runtime")
}

/// MIR3 AI Core 发行版安装目录
pub fn get_dsh_install_path<R: Runtime>(app_handle: &AppHandle<R>) -> PathBuf {
    get_base_dir(app_handle)
        .join("dependencies")
        .join(DSH_CORE_DIR)
}

/// dsh CLI 入口
pub fn get_dsh_binary_path<R: Runtime>(app_handle: &AppHandle<R>) -> PathBuf {
    get_dsh_install_path(app_handle).join(super::core_compat::CORE_ENTRY_RELATIVE)
}

/// pnpm 安装目录
pub fn get_pnpm_install_path<R: Runtime>(app_handle: &AppHandle<R>) -> PathBuf {
    get_base_dir(app_handle)
        .join("dependencies")
        .join(PNPM_CORE_DIR)
}

/// 捆绑 pnpm CLI 入口（纯 JS 发行，用 node 运行）
pub fn get_pnpm_binary_path<R: Runtime>(app_handle: &AppHandle<R>) -> PathBuf {
    get_pnpm_install_path(app_handle).join(PNPM_ENTRY_RELATIVE)
}

/// pnpm 官方/镜像下载前缀：国内走 npmmirror registry，其他直连 npmjs.org
fn pnpm_base_url(region: Region) -> &'static str {
    match region {
        Region::Domestic => PNPM_MIRROR_BASE_URL,
        Region::Overseas => PNPM_BASE_URL,
    }
}

/// pnpm 下载地址（纯 JS 发行，全平台同一 URL）
pub fn get_pnpm_download_url() -> String {
    format!(
        "{}pnpm-{}.tgz",
        pnpm_base_url(detect_region()),
        PNPM_VERSION
    )
}

/// MIR3 AI Core 发行版清单路径
pub fn get_dsh_package_json_path<R: Runtime>(app_handle: &AppHandle<R>) -> PathBuf {
    get_dsh_install_path(app_handle).join(DSH_MANIFEST_RELATIVE)
}

/// 用户主目录（Windows 取 `%USERPROFILE%`，Unix 取 `$HOME`）。
///
/// 不使用 dirs crate（未引入该依赖），与官方 dsh 的 `$HOME/.dsh` 语义保持一致。
fn user_home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let key = "USERPROFILE";
    #[cfg(not(windows))]
    let key = "HOME";
    std::env::var_os(key).map(PathBuf::from)
}

/// MIR3 Studio AI 用户数据目录。
///
/// 对外只接受 `MIR3_STUDIO_HOME`。启动兼容核心子进程时，工作流模块会把此
/// 路径映射为核心协议要求的内部环境变量。
pub fn get_dsh_data_path<R: Runtime>(_app_handle: &AppHandle<R>) -> PathBuf {
    let brand = super::brand::get();
    if let Some(home) = std::env::var_os(&brand.home_env) {
        if !home.is_empty() {
            return PathBuf::from(home);
        }
    }
    let dir_name = if cfg!(debug_assertions) {
        &brand.dev_data_dir
    } else {
        &brand.data_dir
    };
    user_home_dir()
        .map(|home| home.join(dir_name))
        .unwrap_or_else(|| PathBuf::from(dir_name))
}

/// dsh 服务日志文件路径
///
/// debug 构建写入独立的 `dsh-web.dev.log`：开发版每次启动都会轮转日志，若与
/// 生产共用同一个文件，会把正在运行的生产版日志记录轮转覆盖掉。
pub fn get_service_log_path<R: Runtime>(app_handle: &AppHandle<R>) -> PathBuf {
    let name = if cfg!(debug_assertions) {
        "dsh-web.dev.log"
    } else {
        "dsh-web.log"
    };
    get_base_dir(app_handle).join("logs").join(name)
}

/// 捆绑的 Node.js 版本号
pub fn get_bundled_node_version() -> String {
    NODE_VERSION.trim_start_matches('v').to_string()
}

/// 当前实际使用的 Node.js 版本号（本地 Node 优先，其次捆绑运行时）
pub fn get_active_node_version() -> String {
    if let Some(local_node) = get_local_node_path() {
        if let Some(version) = get_node_version_of(&local_node) {
            return version;
        }
    }
    get_bundled_node_version()
}

fn parse_node_version(output: &str) -> Option<(u64, u64, u64)> {
    let version = output.trim().trim_start_matches('v');
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// 兼容性规则：v22.15.0+ 或 v23.8.0+（v24+ 也满足）
fn is_supported_node_version(version: &str) -> bool {
    let Some((major, minor, _patch)) = parse_node_version(version) else {
        return false;
    };
    match major {
        22 => minor >= 15,
        23 => minor >= 8,
        major if major >= 24 => true,
        _ => false,
    }
}

/// 运行 `node --version` 并判断运行时是否兼容
pub fn is_runtime_compatible(app_handle: &tauri::AppHandle) -> bool {
    let node = get_node_binary_path(app_handle);
    if !node.exists() {
        return false;
    }
    let output = match node_version_output(&node) {
        Some(out) => out,
        None => return false,
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    is_supported_node_version(stdout.trim())
}

/// 从打包的 MIR3 AI Core 清单读取 dsh 版本（界面展示用）
pub fn get_dsh_version<R: Runtime>(app_handle: &AppHandle<R>) -> Option<String> {
    let manifest_path = get_dsh_package_json_path(app_handle);
    let content = fs::read_to_string(&manifest_path).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&content).ok()?;
    manifest
        .get("dependencies")
        .and_then(|deps| deps.get(super::core_compat::CORE_PACKAGE))
        .and_then(|value| value.as_str())
        .map(|value| {
            value
                .trim_start_matches(['^', '~', '=', '>', '<'])
                .to_string()
        })
}

/// 侧边栏展示的运行时/版本/诊断信息
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeInfo {
    pub app_version: String,
    pub dsh_version: Option<String>,
    pub node_version: String,
    pub service_url: String,
    pub data_dir: String,
    pub log_path: String,
    pub platform: String,
    pub arch: String,
}

pub fn runtime_info<R: Runtime>(app: &AppHandle<R>, port: u16) -> RuntimeInfo {
    RuntimeInfo {
        app_version: app.package_info().version.to_string(),
        dsh_version: get_dsh_version(app),
        node_version: get_active_node_version(),
        service_url: get_dsh_service_url(port),
        // 用户数据所在目录 = $MIR3_STUDIO_HOME（release 为官方 ~/.mir3-studio-ai，debug 为独立
        // ~/.mir3-studio-ai.dev，见 get_dsh_data_path），不再是 AppData
        data_dir: get_dsh_data_path(app).to_string_lossy().into_owned(),
        log_path: get_service_log_path(app).to_string_lossy().into_owned(),
        platform: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_base_url_switches_on_region() {
        assert_eq!(node_base_url(Region::Overseas), NODE_BASE_URL);
        assert_eq!(node_base_url(Region::Domestic), NODE_MIRROR_BASE_URL);
    }

    #[test]
    fn dsh_download_urls_prefer_official_then_mirror() {
        // 无论哪个地域，首选源都是 GitHub 官方直连；镜像仅作兜底
        let urls = get_dsh_download_urls().expect("dsh urls");
        assert_eq!(urls.len(), 2);
        assert!(
            urls[0].starts_with(crate::config::core_compat::CORE_RELEASE_BASE),
            "first source must be official GitHub: {}",
            urls[0]
        );
        assert!(
            urls[1].starts_with(DSH_MIRROR_PREFIX),
            "fallback must be ghfast mirror: {}",
            urls[1]
        );
        // 两个源的文件名一致（镜像只是换前缀，解压类型判定不受影响）
        let name = |u: &str| u.rsplit('/').next().unwrap_or("").to_string();
        assert_eq!(name(&urls[0]), name(&urls[1]));
    }

    #[test]
    fn mirror_url_prepends_ghfast_prefix() {
        let asset = format!(
            "{}/releases/download/v1.0.0/{}",
            crate::config::core_compat::CORE_RELEASE_REPO,
            crate::config::core_compat::asset_filename("windows", "x86_64").unwrap()
        );
        assert_eq!(
            mirror_download_url(&asset),
            format!("{DSH_MIRROR_PREFIX}{asset}")
        );
    }

    #[test]
    fn pnpm_base_url_switches_on_region() {
        assert_eq!(pnpm_base_url(Region::Overseas), PNPM_BASE_URL);
        assert_eq!(pnpm_base_url(Region::Domestic), PNPM_MIRROR_BASE_URL);
    }

    #[test]
    fn download_urls_keep_platform_filename_shape() {
        // 无论哪个地域，URL 都以 https 开头并保留平台文件名（镜像只是换前缀）
        let node = get_node_download_url().expect("node url");
        assert!(node.starts_with("https://"));
        let filename = node.rsplit('/').next().expect("node url filename");
        assert!(filename.starts_with(&format!("node-{}", NODE_VERSION)));
        assert!(filename.ends_with(".zip") || filename.ends_with(".tar.gz"));

        let dsh = get_dsh_download_url().expect("dsh url");
        assert!(dsh.starts_with("https://"));
        assert!(dsh.ends_with(".zip"));
    }
}
