// 全局导航状态：供 App 与各视图共享切换
import { ref } from "vue";

export type AppTab = "library" | "collect" | "archive" | "tasks" | "settings";

export const currentTab = ref<AppTab>("library");

/** 切换主导航到指定页。 */
export function navigateTo(tab: AppTab): void {
  currentTab.value = tab;
}

export function useNav() {
  return { currentTab, navigateTo };
}
