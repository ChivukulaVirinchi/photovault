import { describe, expect, it } from "vitest";

import { commandErrorMessage } from "./index";

describe("commandErrorMessage", () => {
  it("explains newer-schema library failures with the update path", () => {
    expect(
      commandErrorMessage({
        kind: "schema_too_new",
        db_version: 31,
        max_supported: 27,
      }),
    ).toBe("This library uses a newer Smriti schema (v31). Update Smriti from Settings, then open it again.");
  });

  it("keeps untagged Error messages readable", () => {
    expect(commandErrorMessage(new Error("plain failure"))).toBe("plain failure");
    expect(commandErrorMessage({ message: "worker failed" })).toBe("worker failed");
  });
});
