import { createApp } from "vue";
import App from "./App.vue";
import "./styles.css";

/** 关闭 macOS WebView 对英文首字母的自动大写 / 自动纠正 */
function disableInputAutoCapitalize(root: ParentNode = document) {
  root.querySelectorAll("input, textarea").forEach((node) => {
    if (
      !(node instanceof HTMLInputElement) &&
      !(node instanceof HTMLTextAreaElement)
    ) {
      return;
    }
    const type = node instanceof HTMLInputElement ? node.type : "text";
    if (
      type === "checkbox" ||
      type === "radio" ||
      type === "button" ||
      type === "submit" ||
      type === "file" ||
      type === "hidden"
    ) {
      return;
    }
    node.setAttribute("autocapitalize", "none");
    node.setAttribute("autocorrect", "off");
    node.setAttribute("spellcheck", "false");
    // 技术字段避免浏览器改写大小写
    if (!node.autocomplete || node.autocomplete === "on") {
      node.setAttribute("autocomplete", "off");
    }
  });
}

createApp(App).mount("#app");

disableInputAutoCapitalize();
const observer = new MutationObserver((mutations) => {
  for (const m of mutations) {
    for (const node of m.addedNodes) {
      if (node instanceof HTMLElement) {
        disableInputAutoCapitalize(node);
      }
    }
  }
});
observer.observe(document.body, { childList: true, subtree: true });
