import Sortable, { type SortableEvent } from "sortablejs";
import { nextTick, onBeforeUnmount, watch, type Ref } from "vue";

/** Drag handle class — put on the move trigger. */
export const SORTABLE_ROW_HANDLE_CLASS = "sortableRowHandle";

export function useSortableReorder(opts: {
  containerRef: Ref<HTMLElement | null>;
  draggable?: string;
  handle?: string | false;
  filter?: string;
  preferDraggableIndex?: boolean;
  enabled?: Ref<boolean>;
  active?: Ref<boolean>;
  itemCount?: Ref<number>;
  onReorder: (fromIndex: number, toIndex: number) => void;
}) {
  let sortable: Sortable | null = null;

  function destroy() {
    sortable?.destroy();
    sortable = null;
  }

  function syncDisabled() {
    sortable?.option("disabled", opts.enabled?.value === false);
  }

  function mount() {
    if (opts.active?.value === false) {
      destroy();
      return;
    }
    const el = opts.containerRef.value;
    if (!el) {
      destroy();
      return;
    }
    destroy();
    sortable = Sortable.create(el, {
      ...(opts.handle === false
        ? {}
        : { handle: opts.handle ?? `.${SORTABLE_ROW_HANDLE_CLASS}` }),
      draggable: opts.draggable ?? "tr",
      ...(opts.filter ? { filter: opts.filter, preventOnFilter: true } : {}),
      // Native HTML5 DnD breaks for <tr> inside overflow scrollports; fallback is reliable.
      forceFallback: true,
      fallbackOnBody: true,
      fallbackTolerance: 3,
      animation: 150,
      ghostClass: "sortableRowGhost",
      chosenClass: "sortableRowChosen",
      dragClass: "sortableRowDrag",
      onEnd(evt: SortableEvent) {
        const oldIndex = opts.preferDraggableIndex
          ? evt.oldDraggableIndex
          : evt.oldIndex;
        const newIndex = opts.preferDraggableIndex
          ? evt.newDraggableIndex
          : evt.newIndex;
        if (oldIndex == null || newIndex == null || oldIndex === newIndex) return;
        opts.onReorder(oldIndex, newIndex);
      },
    });
    syncDisabled();
  }

  function scheduleMount() {
    void nextTick(mount);
  }

  watch(() => opts.containerRef.value, scheduleMount);
  if (opts.active) watch(opts.active, scheduleMount);
  if (opts.enabled) watch(opts.enabled, syncDisabled);
  if (opts.itemCount) watch(opts.itemCount, scheduleMount);

  onBeforeUnmount(destroy);

  return { remount: scheduleMount };
}
