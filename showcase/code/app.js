// Keeps the find bar in step with the page, and remembers the last query.

const STORAGE_KEY = 'reposerve:find';

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

export function createFinder(root = document.body) {
  let query = sessionStorage.getItem(STORAGE_KEY) ?? '';

  function matches() {
    if (!query) return [];
    const pattern = new RegExp(escapeRegExp(query), 'gi');
    const found = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);

    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      for (const hit of node.data.matchAll(pattern)) {
        const range = document.createRange();
        range.setStart(node, hit.index);
        range.setEnd(node, hit.index + hit[0].length);
        found.push(range);
      }
    }
    return found;
  }

  return {
    get query() {
      return query;
    },
    set query(value) {
      query = value;
      sessionStorage.setItem(STORAGE_KEY, value);
    },
    count() {
      return matches().length;
    },
  };
}
