//! MIR3 AI Core 与当前上游包协议的唯一兼容边界。
//!
//! 这里的包名、发行资产名和环境变量是实现细节，不属于产品品牌或公开接口。

pub const CORE_PACKAGE: &str = "@deepseek-ai/dsh";
pub const CORE_SCOPE: &str = "@deepseek-ai";
pub const CORE_PACKAGE_NAME: &str = "dsh";
pub const CORE_ENTRY_RELATIVE: &str = "node_modules/@deepseek-ai/dsh/lib/bin.js";
pub const WEB_PROFILE_BUNDLES: [&str; 2] = ["@deepseek-ai/dsh-base", "@deepseek-ai/dsh-web-app"];
pub const CORE_RELEASE_REPO: &str = "https://github.com/hairyf/deepseek-harness-pkg";
pub const CORE_RELEASE_API: &str = "https://api.github.com/repos/hairyf/deepseek-harness-pkg";
pub const CORE_RELEASE_BASE: &str =
    "https://github.com/hairyf/deepseek-harness-pkg/releases/latest/download/";
pub const CORE_RELEASE_MIRROR_BASE: &str =
    "https://ghfast.top/https://github.com/hairyf/deepseek-harness-pkg/releases/latest/download/";
pub const CORE_HOME_ENV: &str = "DSH_HOME";

pub fn is_official_package(name: &str) -> bool {
    name.starts_with(&format!("{CORE_SCOPE}/"))
}

pub fn latest_install_spec() -> String {
    format!("{CORE_PACKAGE}@latest")
}

/// 为兼容核心渲染 Windows 极简 preset。包名只在此兼容边界维护。
#[cfg(windows)]
pub fn render_windows_minimal_composition(shell_path: &str) -> String {
    let shell_path = shell_path.replace('\'', "''");
    format!(
        r#"# Windows minimal preset for the current MIR3 AI Core compatibility layer.
- id: persona
  name: '@deepseek-ai/dsh-persona'
  config:
    text: You are a helpful software engineer assistant.
    complete: true
    includeRuntimeContext: false

- id: persistent-shell
  name: cordis:group
  group: true
  isolate:
    terminals: true
    sandboxPolicy: true
  config:
    - id: pty
      name: '@deepseek-ai/dsh-terminal'
    - id: sandbox-policy
      name: '@deepseek-ai/dsh-sandbox-policy'
      config:
        mode: danger-full-access
        workspaceRoot: !!js process.env.DSH_CWD ?? process.cwd()
    - id: terminal-bash
      name: '@deepseek-ai/dsh-terminal-bash'
      config:
        timeoutMs: 300000
        shellPath: '{}'
        shellArgs: ['--noprofile', '--norc', '-i']
    - id: persistent-bash
      name: '@deepseek-ai/dsh-tool-bash-persistent'
      config:
        timeoutMs: 300000
        description: |-
          Run commands in a bash shell (Git Bash on Windows)
          * This shell runs unconfined (danger-full-access): no file sandbox on shell commands.
          * State is persistent across command calls and discussions with the user.

- id: filesystem
  name: cordis:group
  group: true
  isolate:
    fs: true
  config:
    - id: fs-local
      name: '@deepseek-ai/dsh-fs-local'
      config:
        cwd: !!js process.env.DSH_CWD ?? process.cwd()
    - id: str-replace-editor
      name: '@deepseek-ai/dsh-tool-str-replace-editor'
      config:
        maxOutputChars: 16000
"#,
        shell_path
    )
}

pub fn asset_filename(os: &str, arch: &str) -> Result<&'static str, String> {
    match (os, arch) {
        ("windows", _) => Ok("deepseek-harness-pkg-windows.zip"),
        ("macos", "aarch64") => Ok("deepseek-harness-pkg-macos-arm64.zip"),
        ("macos", "x86_64") => Ok("deepseek-harness-pkg-macos-x64.zip"),
        ("linux", _) => Ok("deepseek-harness-pkg-linux.zip"),
        _ => Err(format!("Unsupported platform: {os} {arch}")),
    }
}
