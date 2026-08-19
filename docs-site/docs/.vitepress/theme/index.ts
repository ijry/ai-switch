import DefaultTheme from "vitepress/theme";
import type { Theme } from "vitepress";
import { h } from "vue";
import HomePage from "./HomePage.vue";
import RelayPage from "./RelayPage.vue";
import "./style.css";

export default {
  extends: DefaultTheme,
  Layout() {
    // VPHome renders home-features-after unconditionally, so index.md needs
    // nothing beyond `layout: home` for this to mount.
    return h(DefaultTheme.Layout, null, {
      "home-features-after": () => h(HomePage),
    });
  },
  enhanceApp({ app }) {
    // relay/index.md is `layout: page` and contains only <RelayPage />, so the
    // component has to be registered globally for the Markdown to resolve it.
    app.component("RelayPage", RelayPage);
  },
} satisfies Theme;
