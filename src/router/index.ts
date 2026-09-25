import { createRouter, createWebHashHistory } from "vue-router";

/**
 * 使用 hash 模式：Tauri 打包后以 `tauri://localhost` 加载本地资源，
 * hash 模式无需服务端 rewrite，最稳妥。
 */
export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: "/",
      name: "library",
      component: () => import("../views/LibraryView.vue"),
      meta: { title: "游戏库" },
    },
    {
      path: "/game/:id",
      name: "game",
      component: () => import("../views/GameDetailView.vue"),
      props: true,
      meta: { title: "游戏详情" },
    },
    {
      path: "/stats",
      name: "stats",
      component: () => import("../views/StatsView.vue"),
      meta: { title: "数据统计" },
    },
    {
      path: "/settings",
      name: "settings",
      component: () => import("../views/SettingsView.vue"),
      meta: { title: "设置" },
    },
    {
      path: "/about",
      name: "about",
      component: () => import("../views/AboutView.vue"),
      meta: { title: "关于" },
    },
    { path: "/:pathMatch(.*)*", redirect: "/" },
  ],
});

router.afterEach((to) => {
  const title = (to.meta.title as string) ?? "";
  document.title = title ? `GalManager · ${title}` : "GalManager";
});
