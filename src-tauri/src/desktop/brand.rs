//! 内嵌核心静态界面的 MIR3 产品桥。
//!
//! 只处理导航、标题、首次模型提示与设置面板等宿主级静态节点；消息、输入框、
//! 代码块和用户内容明确排除，避免改变模型输出或项目文本。Harness 的设置组件、
//! 数据读写和插件能力保持原样，只改变其在 MIR3 Studio 壳层中的入口与呈现方式。

pub(crate) const IFRAME_BRAND_JS: &str = r#"(function () {
  if (window.__mir3_brand_bridge__) return;
  window.__mir3_brand_bridge__ = true;

  // 该初始化脚本也会进入 Tauri 主文档；产品桥只接管内嵌 Core 页面。
  if (window.parent === window) return;

  var STATIC_ROOTS = 'header, nav, aside, [role="dialog"], [data-brand], [data-onboarding]';
  var USER_CONTENT = 'input, textarea, pre, code, [contenteditable], article, [role="log"], [role="listitem"], [data-message], .message, .markdown, .prose';
  var NEW_SESSION_LABELS = ['新建会话', 'New session'];
  var TOGGLE_LABELS = ['打开侧边栏', '收起侧边栏', 'Open sidebar', 'Collapse sidebar'];
  var HERO_HEADLINES = ['探索未至之境', 'Into the Unknown'];
  var HERO_BADGES = ['预览版', 'Preview'];
  var SETTINGS_LABELS = ['设置', 'Settings'];
  var WELCOME_TITLES = ['内测声明', 'Internal Testing Notice'];
  var WELCOME_CONTINUE = ['继续', 'Continue'];
  var ONBOARDING_TITLES = ['添加一个 API Key 开始使用', 'Add an API key to get started'];
  var ONBOARDING_LATER = ['稍后配置', 'Configure later'];
  var LEGACY_CORE_NAME = ['DeepSeek', 'Harness'].join(' ');
  var TEXT_MAP = {};
  TEXT_MAP[LEGACY_CORE_NAME] = 'MIR3 AI Core';
  TEXT_MAP[LEGACY_CORE_NAME + ' Core'] = 'MIR3 AI Core';
  Object.assign(TEXT_MAP, {
    '配置 DeepSeek 官方模型': '配置 AI 模型'
  });
  var LOGO = 'data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 64 64%22%3E%3Crect width=%2264%22 height=%2264%22 rx=%2214%22 fill=%22%23090909%22/%3E%3Cpath d=%22M12 46V18h8l12 15 12-15h8v28h-8V30L32 45 20 30v16z%22 fill=%22%23d7ad57%22/%3E%3C/svg%3E';
  var desiredSurface = 'workbench';
  var settingsOpening = false;

  function textOf(element) {
    return (element && element.textContent || '').replace(/\s+/g, ' ').trim();
  }

  function hasExactText(element, choices) {
    return choices.indexOf(textOf(element)) !== -1;
  }

  function findFrame() {
    var overlay = document.querySelector('[data-shell-overlay]');
    return overlay && overlay.parentElement;
  }

  function isStatic(node) {
    var parent = node.parentElement;
    return !!parent && !!parent.closest(STATIC_ROOTS) && !parent.closest(USER_CONTENT);
  }

  function replaceText(root) {
    var walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    var node;
    while ((node = walker.nextNode())) {
      var trimmed = node.nodeValue && node.nodeValue.trim();
      if (trimmed && TEXT_MAP[trimmed] && isStatic(node)) {
        node.nodeValue = node.nodeValue.replace(trimmed, TEXT_MAP[trimmed]);
      }
    }
  }

  function replaceLogos(root) {
    root.querySelectorAll('img[alt], img[title]').forEach(function (img) {
      if (img.closest(USER_CONTENT)) return;
      var label = (img.getAttribute('alt') || img.getAttribute('title') || '').trim();
      if (label === LEGACY_CORE_NAME || label === LEGACY_CORE_NAME + ' Core') {
        img.src = LOGO;
        img.alt = 'MIR3 AI Core';
        img.title = 'MIR3 AI Core';
      }
    });
  }

  // 移除 Core 内部的产品品牌占位，但保留侧边栏折叠和新建会话能力。
  function annotateSidebarBrand() {
    var frame = findFrame();
    var column = frame && frame.firstElementChild;
    if (!column) return;
    var buttons = Array.prototype.slice.call(column.querySelectorAll('button'));
    var sessionButtons = buttons.filter(function (button) {
      return NEW_SESSION_LABELS.indexOf((button.getAttribute('aria-label') || '').trim()) !== -1;
    });
    // 展开态有两个同名按钮：第一个是品牌按钮，第二个才是“新建会话”。
    if (sessionButtons.length > 1) {
      sessionButtons[0].setAttribute('data-mir3-sidebar-brand', '');
      if (sessionButtons[0].parentElement) {
        sessionButtons[0].parentElement.setAttribute('data-mir3-sidebar-brand-row', '');
      }
    }
    var toggle = buttons.find(function (button) {
      return TOGGLE_LABELS.indexOf((button.getAttribute('aria-label') || '').trim()) !== -1;
    });
    if (!toggle) return;
    toggle.setAttribute('data-mir3-sidebar-toggle', '');
    Array.prototype.forEach.call(toggle.children, function (child) {
      if (child.tagName === 'SPAN' && child.getAttribute('aria-hidden') === 'true') {
        child.setAttribute('data-mir3-sidebar-mark', '');
      }
    });
  }

  // 欢迎页只移除品牌标题行；输入区、工作区选择和 Agent 控件继续可用。
  function annotateHeroBrand() {
    var candidates = document.querySelectorAll('span');
    for (var i = 0; i < candidates.length; i++) {
      var headline = candidates[i];
      if (!hasExactText(headline, HERO_HEADLINES) || headline.closest(USER_CONTENT)) continue;
      var row = headline.parentElement;
      if (!row) continue;
      var descendants = row.querySelectorAll('span');
      var hasBadge = Array.prototype.some.call(descendants, function (node) {
        return hasExactText(node, HERO_BADGES);
      });
      if (hasBadge) row.setAttribute('data-mir3-hero-brand', '');
    }
  }

  function findSettingsTrigger() {
    var buttons = document.querySelectorAll('button[aria-haspopup="dialog"]');
    for (var i = 0; i < buttons.length; i++) {
      if (hasExactText(buttons[i], SETTINGS_LABELS)) return buttons[i];
    }
    return null;
  }

  function dialogLabel(dialog) {
    var labelId = dialog && dialog.getAttribute('aria-labelledby');
    return labelId ? textOf(document.getElementById(labelId)) : '';
  }

  function findSettingsPanel() {
    var dialogs = document.querySelectorAll('[role="dialog"]');
    for (var i = 0; i < dialogs.length; i++) {
      if (SETTINGS_LABELS.indexOf(dialogLabel(dialogs[i])) !== -1) return dialogs[i];
    }
    return null;
  }

  function findButtonByText(root, labels) {
    if (!root) return null;
    var buttons = root.querySelectorAll('button');
    for (var i = 0; i < buttons.length; i++) {
      if (hasExactText(buttons[i], labels)) return buttons[i];
    }
    return null;
  }

  function annotateSettings() {
    var trigger = findSettingsTrigger();
    if (trigger) trigger.setAttribute('data-mir3-settings-trigger', '');
    var panel = findSettingsPanel();
    if (!panel) return null;
    panel.setAttribute('data-mir3-settings-panel', '');
    var overlay = panel.parentElement;
    if (overlay) {
      overlay.setAttribute('data-mir3-settings-overlay', '');
      Array.prototype.forEach.call(overlay.children, function (child) {
        if (child !== panel && child.getAttribute('aria-hidden') === 'true') {
          child.setAttribute('data-mir3-settings-mask', '');
        }
      });
    }
    var nav = panel.querySelector('nav');
    if (nav) nav.setAttribute('data-mir3-settings-nav', '');
    var close = findButtonByText(panel, ['关闭', 'Close']);
    if (close) close.setAttribute('data-mir3-settings-close', '');
    return panel;
  }

  function hideOnboardingDialog(dialog) {
    var layer = dialog;
    var parent = dialog.parentElement;
    while (parent && parent !== document.body) {
      if (window.getComputedStyle(parent).position === 'fixed') layer = parent;
      parent = parent.parentElement;
    }
    layer.setAttribute('data-mir3-onboarding-hidden', '');
  }

  // 首装欢迎声明使用 Core 自己的 acknowledge 回调落盘，只是不再向 MIR3 用户
  // 展示旧产品的内测弹窗。精确匹配标题和按钮，避免影响其他确认对话框。
  function acknowledgeWelcomeNotice() {
    var dialogs = document.querySelectorAll('[role="dialog"]');
    for (var i = 0; i < dialogs.length; i++) {
      var dialog = dialogs[i];
      if (dialog.hasAttribute('data-mir3-welcome-acknowledging')) continue;
      var heading = dialog.querySelector('h2');
      if (!heading || !hasExactText(heading, WELCOME_TITLES)) continue;
      var continueButton = findButtonByText(dialog, WELCOME_CONTINUE);
      if (!continueButton) continue;
      dialog.setAttribute('data-mir3-welcome-acknowledging', '');
      hideOnboardingDialog(dialog);
      continueButton.click();
    }
  }

  // 精确匹配首次 API Key 步骤并走插件自己的“稍后配置”回调；不隐藏或误点
  // 其他对话框，也不伪造模型配置状态。
  function dismissModelOnboarding() {
    var dialogs = document.querySelectorAll('[role="dialog"]');
    for (var i = 0; i < dialogs.length; i++) {
      var dialog = dialogs[i];
      var heading = dialog.querySelector('h2');
      if (!heading || !hasExactText(heading, ONBOARDING_TITLES)) continue;
      var later = findButtonByText(dialog, ONBOARDING_LATER);
      if (!later) continue;
      hideOnboardingDialog(dialog);
      later.click();
    }
  }

  function openSettings() {
    if (findSettingsPanel() || settingsOpening) return;
    var trigger = findSettingsTrigger();
    if (!trigger) return;
    settingsOpening = true;
    trigger.click();
    window.setTimeout(function () {
      settingsOpening = false;
      applyChrome();
    }, 80);
  }

  function closeSettings() {
    var panel = annotateSettings();
    if (!panel) return;
    var close = panel.querySelector('[data-mir3-settings-close]') || findButtonByText(panel, ['关闭', 'Close']);
    var overlay = panel.parentElement;
    if (overlay) overlay.setAttribute('data-mir3-settings-closing', '');
    if (close) close.click();
  }

  function syncSurface() {
    document.documentElement.setAttribute('data-mir3-surface', desiredSurface);
    if (desiredSurface === 'settings') openSettings();
    else closeSettings();
  }

  function applyChrome() {
    annotateSidebarBrand();
    annotateHeroBrand();
    annotateSettings();
    acknowledgeWelcomeNotice();
    dismissModelOnboarding();
    syncSurface();
  }

  function apply(root) {
    if (!root || root.nodeType !== Node.ELEMENT_NODE) return;
    document.title = 'MIR3 AI Core';
    replaceText(root);
    replaceLogos(root);
    applyChrome();
  }

  function start() {
    apply(document.documentElement);
    new MutationObserver(function (mutations) {
      mutations.forEach(function (mutation) {
        mutation.addedNodes.forEach(function (node) {
          if (node.nodeType === Node.ELEMENT_NODE) apply(node);
        });
      });
    }).observe(document.documentElement, { childList: true, subtree: true });
  }

  window.addEventListener('message', function (event) {
    if (event.source !== window.parent) return;
    var data = event.data;
    if (!data || data.source !== 'dsh-desktop' || data.type !== 'mir3://surface:set') return;
    desiredSurface = data.surface === 'settings' ? 'settings' : 'workbench';
    syncSurface();
  });

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_has_user_content_exclusions() {
        for selector in [
            "input",
            "textarea",
            "pre",
            "code",
            "[contenteditable]",
            "[data-message]",
        ] {
            assert!(IFRAME_BRAND_JS.contains(selector));
        }
    }

    #[test]
    fn bridge_only_replaces_exact_static_brand_text() {
        assert!(IFRAME_BRAND_JS.contains("TEXT_MAP[LEGACY_CORE_NAME] = 'MIR3 AI Core'"));
        assert!(IFRAME_BRAND_JS.contains("node.nodeValue.trim()"));
        assert!(IFRAME_BRAND_JS.contains("parent.closest(STATIC_ROOTS)"));
    }

    #[test]
    fn bridge_dismisses_only_the_exact_model_onboarding_step() {
        assert!(IFRAME_BRAND_JS.contains("ONBOARDING_TITLES"));
        assert!(IFRAME_BRAND_JS.contains("ONBOARDING_LATER"));
        assert!(IFRAME_BRAND_JS.contains("hasExactText(heading, ONBOARDING_TITLES)"));
        assert!(IFRAME_BRAND_JS.contains("findButtonByText(dialog, ONBOARDING_LATER)"));
    }

    #[test]
    fn bridge_acknowledges_only_the_exact_welcome_notice() {
        assert!(IFRAME_BRAND_JS.contains("WELCOME_TITLES = ['内测声明'"));
        assert!(IFRAME_BRAND_JS.contains("WELCOME_CONTINUE = ['继续'"));
        assert!(IFRAME_BRAND_JS.contains("hasExactText(heading, WELCOME_TITLES)"));
        assert!(IFRAME_BRAND_JS.contains("findButtonByText(dialog, WELCOME_CONTINUE)"));
        assert!(IFRAME_BRAND_JS.contains("continueButton.click()"));
    }

    #[test]
    fn bridge_exposes_a_settings_surface_without_copying_settings_logic() {
        assert!(IFRAME_BRAND_JS.contains("mir3://surface:set"));
        assert!(IFRAME_BRAND_JS.contains("data-mir3-settings-panel"));
        assert!(IFRAME_BRAND_JS.contains("trigger.click()"));
        assert!(IFRAME_BRAND_JS.contains("close.click()"));
    }
}
