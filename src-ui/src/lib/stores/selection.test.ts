import { beforeEach, describe, expect, it } from "vitest";

import { handleCellClick, selection } from "./selection.svelte";

beforeEach(() => {
  selection.clear();
});

/// Build a mock MouseEvent with just the modifier flags + the methods
/// our code touches. handleCellClick only reads shiftKey/ctrlKey/
/// metaKey and calls preventDefault.
function mouseEvent(opts: {
  shift?: boolean;
  ctrl?: boolean;
  meta?: boolean;
}): MouseEvent {
  const e = {
    shiftKey: !!opts.shift,
    ctrlKey: !!opts.ctrl,
    metaKey: !!opts.meta,
    preventDefault: () => {},
  } as unknown as MouseEvent;
  return e;
}

describe("selection store", () => {
  it("toggle adds an unselected id and removes a selected one", () => {
    selection.toggle(7);
    expect(selection.list()).toEqual([7]);
    selection.toggle(7);
    expect(selection.list()).toEqual([]);
  });

  it("set replaces the entire selection with a single id", () => {
    selection.toggle(1);
    selection.toggle(2);
    selection.set(99);
    expect(selection.list()).toEqual([99]);
    expect(selection.anchor).toBe(99);
  });

  it("replace keeps a valid anchor when replacing wholesale", () => {
    selection.set(1);
    selection.replace([2, 3]);
    expect(selection.list()).toEqual([2, 3]);
    expect(selection.anchor).toBe(2);
  });

  it("replace preserves the anchor when it is still selected", () => {
    selection.set(2);
    selection.replace([1, 2, 3]);
    expect(selection.anchor).toBe(2);
  });

  it("range selects an inclusive span from anchor to target", () => {
    const all = [10, 20, 30, 40, 50];
    selection.set(20);
    selection.range(40, all);
    expect(selection.list().sort((a, b) => a - b)).toEqual([20, 30, 40]);
  });

  it("range degrades to single-id select when there is no anchor", () => {
    selection.range(30, [10, 20, 30, 40]);
    expect(selection.list()).toEqual([30]);
  });

  it("clear resets ids and anchor", () => {
    selection.set(7);
    selection.clear();
    expect(selection.list()).toEqual([]);
    expect(selection.anchor).toBeNull();
  });

  it("size + active reflect the current selection", () => {
    expect(selection.active()).toBe(false);
    selection.toggle(1);
    expect(selection.active()).toBe(true);
    expect(selection.size()).toBe(1);
  });
});

describe("handleCellClick", () => {
  it("returns false for an ordinary click on an empty selection (lets nav proceed)", () => {
    expect(handleCellClick(mouseEvent({}), 5, [1, 5, 9])).toBe(false);
    expect(selection.list()).toEqual([]);
  });

  it("returns true and toggles on Ctrl-click", () => {
    expect(handleCellClick(mouseEvent({ ctrl: true }), 5, [1, 5, 9])).toBe(true);
    expect(selection.list()).toEqual([5]);
  });

  it("returns true and toggles on Meta-click", () => {
    expect(handleCellClick(mouseEvent({ meta: true }), 9, [1, 5, 9])).toBe(true);
    expect(selection.list()).toEqual([9]);
  });

  it("returns true and shift-range-selects from anchor", () => {
    selection.set(1);
    expect(handleCellClick(mouseEvent({ shift: true }), 9, [1, 5, 9])).toBe(true);
    expect(selection.list().sort((a, b) => a - b)).toEqual([1, 5, 9]);
  });

  it("toggles instead of navigating when selection is already active", () => {
    selection.set(1);
    expect(handleCellClick(mouseEvent({}), 5, [1, 5, 9])).toBe(true);
    expect(selection.list().sort((a, b) => a - b)).toEqual([1, 5]);
  });
});
