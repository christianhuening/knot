import { beforeEach, describe, expect, it } from "vitest";

import { useUi } from "./ui";

describe("ui theme", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
    useUi.getState().setTheme("light");
  });

  it("toggles between light and dark", () => {
    expect(useUi.getState().theme).toBe("light");
    useUi.getState().toggleTheme();
    expect(useUi.getState().theme).toBe("dark");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    useUi.getState().toggleTheme();
    expect(useUi.getState().theme).toBe("light");
    expect(document.documentElement.getAttribute("data-theme")).toBe("light");
  });

  it("persists to localStorage", () => {
    useUi.getState().setTheme("dark");
    expect(localStorage.getItem("knot.theme")).toBe("dark");
    useUi.getState().setTheme("light");
    expect(localStorage.getItem("knot.theme")).toBe("light");
  });
});
