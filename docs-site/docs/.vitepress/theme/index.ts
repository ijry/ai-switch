import DefaultTheme from "vitepress/theme";
import type { Theme } from "vitepress";
import { h } from "vue";
import HomePage from "./HomePage.vue";
import RelayPage from "./RelayPage.vue";
import RelaySpecPage from "./RelaySpecPage.vue";
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
    // The relay pages are `layout: page` and their Markdown contains only the
    // component tag, so both have to be registered globally to resolve.
    app.component("RelayPage", RelayPage);
    app.component("RelaySpecPage", RelaySpecPage);
  },
} satisfies Theme;
