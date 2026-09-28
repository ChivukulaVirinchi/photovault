// @vitest-environment jsdom
import { expect, it } from "vitest";
import { mount, tick, unmount } from "svelte";
import PageHeader from "./PageHeader.svelte";
import Lazy from "./Lazy.svelte";

it("replaces the route placeholder with the loaded component", async () => {
  const component = mount(Lazy, {
    target: document.body,
    props: {
      load: async () => ({ default: PageHeader }),
      props: { title: "Loaded route" },
      title: "Pending route",
    },
  });
  await tick();
  await Promise.resolve();
  await tick();
  expect(document.body.textContent).toContain("Loaded route");
  expect(document.body.textContent).not.toContain("Pending route");
  await unmount(component);
});
