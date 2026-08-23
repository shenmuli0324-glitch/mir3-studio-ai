//! 内嵌核心静态界面的 MIR3 品牌桥。
//!
//! 只处理导航、标题与 onboarding 等静态节点；消息、输入框、代码块和用户内容
//! 明确排除，避免改变模型输出或项目文本。

pub(crate) const IFRAME_BRAND_JS: &str = r#"(function () {
  if (window.__mir3_brand_bridge__) return;
  window.__mir3_brand_bridge__ = true;

  var STATIC_ROOTS = 'header, nav, aside, [role="dialog"], [data-brand], [data-onboarding]';
  var USER_CONTENT = 'input, textarea, pre, code, [contenteditable], article, [role="log"], [role="listitem"], [data-message], .message, .markdown, .prose';
  var LEGACY_CORE_NAME = ['DeepSeek', 'Harness'].join(' ');
  var TEXT_MAP = {};
  TEXT_MAP[LEGACY_CORE_NAME] = 'MIR3 AI Core';
  TEXT_MAP[LEGACY_CORE_NAME + ' Core'] = 'MIR3 AI Core';
  Object.assign(TEXT_MAP, {
    '配置 DeepSeek 官方模型': '配置 AI 模型'
  });
  var LOGO = 'data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 64 64%22%3E%3Crect width=%2264%22 height=%2264%22 rx=%2214%22 fill=%22%23090909%22/%3E%3Cpath d=%22M12 46V18h8l12 15 12-15h8v28h-8V30L32 45 20 30v16z%22 fill=%22%23d7ad57%22/%3E%3C/svg%3E';

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

  function apply(root) {
    if (!root || root.nodeType !== Node.ELEMENT_NODE) return;
    document.title = 'MIR3 AI Core';
    replaceText(root);
    replaceLogos(root);
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

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_has_user_content_exclusions() {
        for selector in ["input", "textarea", "pre", "code", "[contenteditable]", "[data-message]"] {
            assert!(IFRAME_BRAND_JS.contains(selector));
        }
    }

    #[test]
    fn bridge_only_replaces_exact_static_brand_text() {
        assert!(IFRAME_BRAND_JS.contains("TEXT_MAP[LEGACY_CORE_NAME] = 'MIR3 AI Core'"));
        assert!(IFRAME_BRAND_JS.contains("node.nodeValue.trim()"));
        assert!(IFRAME_BRAND_JS.contains("parent.closest(STATIC_ROOTS)"));
    }
}
