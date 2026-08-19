import DefaultTheme from "vitepress/theme";
import type { Theme } from "vitepress";
import { h } from "vue";
import HomePage from "./HomePage.vue";
import RelayCards from "./RelayCards.vue";
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
    app.component("RelayCards", RelayCards);
  },
} satisfies Theme;
