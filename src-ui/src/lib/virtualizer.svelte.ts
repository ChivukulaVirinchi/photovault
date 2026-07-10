/// Hand-rolled vertical virtualizer.
///
/// Why not @tanstack/svelte-virtual? We tried, and the integration with Svelte
/// 5's `$derived` re-creates the virtualizer on every reactive change which
/// effectively defeats virtualization. The math here is small enough that
/// owning it directly is cheaper than fighting the abstraction.
///
/// Usage:
///   const v = createVirtualScroll({ rows: () => myRows, scrollEl: () => el });
///   onMount(() => v.attach());     // cleans up automatically when component unmounts
///   // then read v.first / v.last / v.offsets / v.totalHeight
///   // and render only rows[v.first .. v.last]

export interface VRow {
  /// Pre-computed pixel height of this row. Caller decides; we just sum.
  height: number;
}

export interface VirtualScroll<R extends VRow> {
  readonly first: number;
  readonly last: number;
  readonly offsets: ReadonlyArray<number>;
  readonly totalHeight: number;
  setScrollTop: (top: number) => void;
  attach: () => () => void;
}

export function createVirtualScroll<R extends VRow>(opts: {
  rows: () => R[];
  scrollEl: () => HTMLElement | null | undefined;
  overscan?: number;
}): VirtualScroll<R> {
  const overscan = opts.overscan ?? 4;

  let scrollTop = $state(0);
  let viewportH = $state(0);

  const metrics = $derived.by(() => {
    const rows = opts.rows();
    const offsets = new Array<number>(rows.length);
    let total = 0;
    for (let i = 0; i < rows.length; i++) {
      offsets[i] = total;
      total += rows[i].height;
    }
    return { rows, offsets, total };
  });

  const window = $derived.by(() => {
    const { rows, offsets, total } = metrics;
    if (rows.length === 0) {
      return { first: 0, last: 0, offsets, total: 0 };
    }

    // Binary search for the first row whose bottom is >= scrollTop.
    let lo = 0;
    let hi = rows.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (offsets[mid] + rows[mid].height < scrollTop) lo = mid + 1;
      else hi = mid;
    }
    const first = Math.max(0, lo - overscan);

    // Linear scan from `first` for the first row whose top >= scrollTop+viewportH.
    const limit = scrollTop + viewportH;
    let last = rows.length;
    for (let i = first; i < rows.length; i++) {
      if (offsets[i] >= limit) {
        last = Math.min(rows.length, i + overscan);
        break;
      }
    }

    return { first, last, offsets, total };
  });

  function attach(): () => void {
    const el = opts.scrollEl();
    if (!el) return () => {};

    let scrollRaf = 0;
    const onScroll = () => {
      if (scrollRaf !== 0) return;
      scrollRaf = requestAnimationFrame(() => {
        scrollRaf = 0;
        scrollTop = el.scrollTop;
      });
    };
    const onResize = () => {
      viewportH = el.clientHeight;
    };

    onResize();
    onScroll();
    el.addEventListener("scroll", onScroll, { passive: true });
    const ro = new ResizeObserver(onResize);
    ro.observe(el);

    return () => {
      if (scrollRaf !== 0) cancelAnimationFrame(scrollRaf);
      el.removeEventListener("scroll", onScroll);
      ro.disconnect();
    };
  }

  return {
    get first() {
      return window.first;
    },
    get last() {
      return window.last;
    },
    get offsets() {
      return window.offsets;
    },
    get totalHeight() {
      return window.total;
    },
    setScrollTop(top: number) {
      scrollTop = Math.max(0, top);
    },
    attach,
  };
}
