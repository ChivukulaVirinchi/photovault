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
    ).toBe("This library was made with a newer version of Smriti. Update Smriti from Settings, then open it again.");
  });

  it("says what failed in plain words, keeping engine detail for developers", () => {
    const error = { kind: "database", message: "SQLITE_BUSY: database is locked" };
    expect(commandErrorMessage(error)).toBe("Smriti couldn't read or save that in the library.");
    expect(commandErrorMessage({ kind: "not_found", entity: "album_suggestion" })).toBe(
      "That album suggestion is no longer here.",
    );
  });

  it("keeps untagged Error messages readable", () => {
    expect(commandErrorMessage(new Error("plain failure"))).toBe("plain failure");
    expect(commandErrorMessage({ message: "worker failed" })).toBe("worker failed");
  });
});
