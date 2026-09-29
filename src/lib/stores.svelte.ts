// 全局状态：用于跨组件通信
// 使用 Svelte 5 的 runes 模式

// 书籍索引（1-based，与 CLI 保持一致）
let _selectedBookIndex = $state<number | null>(null);
let _navigateTo = $state<string | null>(null);

// 只读访问器
export function getSelectedBookIndex() {
  return _selectedBookIndex;
}

export function getNavigateTo() {
  return _navigateTo;
}

// 导航函数：设置书籍索引并跳转到目标页面
export function navigateToPageWithBook(page: string, bookIndex: number) {
  _selectedBookIndex = bookIndex;
  _navigateTo = page;
}

// 清除导航状态（目标页面读取后调用）
export function clearNavigation() {
  _navigateTo = null;
}

// 用于响应式订阅的 getter
export const selectedBookIndex = {
  get value() { return _selectedBookIndex; }
};

export const navigateTo = {
  get value() { return _navigateTo; }
};
