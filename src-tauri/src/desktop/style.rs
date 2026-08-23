//! 注入到内嵌 dsh iframe 的自定义样式桥。
//!
//! 与 [`crate::desktop::nav::NAV_SHIM_JS`] / [`crate::desktop::notification::NOTIFICATION_SHIM_JS`]
//! 走同一套注入通道（Windows 在 FrameCreated → ContentLoading 时 ExecuteScript，
//! 其余平台 `initialization_script_for_all_frames`），因此 iframe 每次重新加载都会重建。
//!
//! 本脚本只负责把一段内置 CSS 以 `<style>` 元素注入 iframe 文档（幂等：按 id 去重）。
//! 具体样式写在下方的 `IFRAME_CSS` 模板字符串里（当前是占位，按需填写/替换即可）。

/// 注入 `<style>` 的脚本（带 `__dsh_iframe_styles__` 幂等守卫，重复注入安全）。
/// 样式直接写在 `css` 模板字符串里，改这里即可。
pub(crate) const IFRAME_STYLES_JS: &str = r#"(function () {
  if (window.__dsh_iframe_styles__) return;
  window.__dsh_iframe_styles__ = true;

  var STYLE_ID = 'dsh-desktop-injected-styles';

  var css = `
    .nArs4W_toggleCluster {top:6px !important; right: 6px !important; gap: 2px !important;}
    .nArs4W_toggleButton {border-radius: 8px !important;}

    [data-mir3-sidebar-brand],
    [data-mir3-sidebar-mark],
    [data-mir3-hero-brand],
    [data-mir3-settings-trigger],
    [data-mir3-onboarding-hidden],
    [data-mir3-settings-closing] {
      display: none !important;
    }

    [data-mir3-sidebar-brand-row] {
      height: 44px !important;
      margin-bottom: 4px !important;
    }

    [data-mir3-sidebar-toggle] > svg {
      display: inline !important;
    }

    html[data-mir3-surface='settings'],
    html[data-mir3-surface='settings'] body,
    html[data-mir3-surface='settings'] #root {
      background: var(--dsw-alias-bg-base) !important;
    }

    html[data-mir3-surface='settings'] [data-mir3-settings-overlay] {
      position: absolute !important;
      inset: 0 !important;
      z-index: 20 !important;
      align-items: stretch !important;
      justify-content: stretch !important;
      background: var(--dsw-alias-bg-base) !important;
    }

    html[data-mir3-surface='settings'] [data-mir3-settings-mask],
    html[data-mir3-surface='settings'] [data-mir3-settings-close] {
      display: none !important;
    }

    html[data-mir3-surface='settings'] [data-mir3-settings-panel] {
      width: 100% !important;
      max-width: none !important;
      height: 100% !important;
      max-height: none !important;
      border-radius: 0 !important;
      box-shadow: none !important;
      background: var(--dsw-alias-bg-base) !important;
    }

    html[data-mir3-surface='settings'] [data-mir3-settings-nav] {
      width: 204px !important;
      padding-top: 24px !important;
      border-right: 1px solid var(--dsw-alias-border-l1) !important;
      background: var(--dsw-specific-sidebar-fill) !important;
    }
  `;

  function apply() {
    if (document.getElementById(STYLE_ID)) return;
    var root = document.head || document.documentElement;
    if (!root) return;
    var style = document.createElement('style');
    style.id = STYLE_ID;
    style.type = 'text/css';
    style.textContent = css;
    root.appendChild(style);
  }

  // 非 Windows 平台在 document-start 注入，head 可能尚未就绪，等 DOM ready 后再挂。
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', apply);
  } else {
    apply();
  }
})();"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_hide_only_annotated_brand_and_onboarding_nodes() {
        assert!(IFRAME_STYLES_JS.contains("[data-mir3-sidebar-brand]"));
        assert!(IFRAME_STYLES_JS.contains("[data-mir3-hero-brand]"));
        assert!(IFRAME_STYLES_JS.contains("[data-mir3-onboarding-hidden]"));
    }

    #[test]
    fn settings_surface_fills_the_existing_iframe_content_area() {
        assert!(IFRAME_STYLES_JS.contains("html[data-mir3-surface='settings']"));
        assert!(IFRAME_STYLES_JS.contains("[data-mir3-settings-panel]"));
        assert!(IFRAME_STYLES_JS.contains("width: 100% !important"));
        assert!(IFRAME_STYLES_JS.contains("height: 100% !important"));
    }
}
