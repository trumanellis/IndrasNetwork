/**
 * Milkdown bridge for Dioxus desktop.
 *
 * Exposes `window.MilkdownBridge` with init/getMarkdown/setMarkdown/destroy
 * methods. The Rust component communicates via `document::eval()`.
 */

import { Editor, rootCtx, defaultValueCtx, editorViewCtx, serializerCtx } from '@milkdown/kit/core';
import { commonmark } from '@milkdown/kit/preset/commonmark';
import { gfm } from '@milkdown/kit/preset/gfm';
import { listener, listenerCtx } from '@milkdown/kit/plugin/listener';
import { history } from '@milkdown/kit/plugin/history';

let editor = null;
let debounceTimer = null;

window.MilkdownBridge = {
  /**
   * Initialize (or re-initialize) the editor in the given container.
   * @param {string} containerId - DOM element id for the editor root
   * @param {string} markdown    - initial markdown content
   */
  async init(containerId, markdown) {
    // Tear down previous instance
    if (editor) {
      try { editor.destroy(); } catch (_) {}
      editor = null;
    }
    clearTimeout(debounceTimer);

    const container = document.getElementById(containerId);
    if (!container) {
      console.error('[MilkdownBridge] container not found:', containerId);
      return;
    }

    // Clear any leftover DOM from a previous editor
    container.innerHTML = '';

    window._milkdownContent = markdown;
    window._milkdownDirty = false;

    editor = await Editor.make()
      .config((ctx) => {
        ctx.set(rootCtx, container);
        ctx.set(defaultValueCtx, markdown);

        const l = ctx.get(listenerCtx);
        l.markdownUpdated((_ctx, md, prevMd) => {
          if (md !== prevMd) {
            window._milkdownContent = md;
            // Debounce the dirty flag so rapid typing doesn't thrash saves
            clearTimeout(debounceTimer);
            debounceTimer = setTimeout(() => {
              window._milkdownDirty = true;
            }, 400);
          }
        });
      })
      .use(listener)
      .use(commonmark)
      .use(gfm)
      .use(history)
      .create();
  },

  /** Return the current markdown content. */
  getMarkdown() {
    if (!editor) return window._milkdownContent || '';
    try {
      const view = editor.ctx.get(editorViewCtx);
      const serializer = editor.ctx.get(serializerCtx);
      return serializer(view.state.doc);
    } catch (_) {
      return window._milkdownContent || '';
    }
  },

  /** Replace the editor content with new markdown. */
  async setMarkdown(markdown) {
    if (!editor) return;
    // Destroy and re-create is the simplest reliable way to set content
    const container = editor.ctx.get(rootCtx);
    const containerId = container?.id;
    if (containerId) {
      await this.init(containerId, markdown);
    }
  },

  /** Tear down the editor. */
  destroy() {
    clearTimeout(debounceTimer);
    if (editor) {
      try { editor.destroy(); } catch (_) {}
      editor = null;
    }
  },
};
