// 全局导航状态：供 App 与各视图共享切换
import { ref } from "vue";

export type AppTab = "library" | "tasks" | "settings";

export const currentTab = ref<AppTab>("library");

/** 跳转文件库时携带的检索关键词（任务历史失败项使用） */
export const libraryFocusQuery = ref<string | null>(null);

/** 切换主导航到指定页。 */
export function navigateTo(tab: AppTab): void {
  currentTab.value = tab;
}

/** 跳转文件库并按关键词检索。 */
export function goToLibraryWithQuery(query: string): void {
  libraryFocusQuery.value = query;
  currentTab.value = "library";
}

export function useNav() {
  return { currentTab, navigateTo, libraryFocusQuery, goToLibraryWithQuery };
}
