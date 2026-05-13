import {
  moveIndex,
  resolveStartIndex,
  shouldLoadMore,
  uniquePhotoIds,
} from "./slideshowQueue.js";

function equal<T>(actual: T, expected: T, label: string) {
  if (actual !== expected) {
    throw new Error(`${label}: expected ${String(expected)}, got ${String(actual)}`);
  }
}

function deepEqual<T>(actual: T, expected: T, label: string) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

deepEqual(uniquePhotoIds([4, 4, 7, Number.NaN, 9, 7]), [4, 7, 9], "dedupes valid ids");
equal(resolveStartIndex([10, 20, 30], 20), 1, "starts at requested id");
equal(resolveStartIndex([10, 20, 30], 99), 0, "falls back to first id");
equal(resolveStartIndex([], 20), -1, "empty queue has no start");

equal(moveIndex(0, 3, "next", true), 1, "advances forward");
equal(moveIndex(2, 3, "next", true), 0, "loops forward");
equal(moveIndex(2, 3, "next", false), 2, "stops forward without loop");
equal(moveIndex(0, 3, "prev", true), 2, "loops backward");
equal(moveIndex(0, 3, "prev", false), 0, "stops backward without loop");
equal(moveIndex(10, 3, "prev", false), 1, "clamps before backward move");

equal(shouldLoadMore(91, 100), false, "does not prefetch outside threshold");
equal(shouldLoadMore(92, 100), true, "prefetches near end");
equal(shouldLoadMore(-1, 0), false, "does not prefetch empty queue");

console.log("slideshowQueue tests passed");
