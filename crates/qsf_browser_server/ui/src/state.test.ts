import { describe, expect, it } from "vitest";
import {
  initialState,
  reduce,
  stateToUrl,
  urlToState,
  type ViewState,
} from "./state";

describe("memory browser URL state", () => {
  it("defaults to recent activity so reinforced memories surface first", () => {
    expect(initialState.query.sort).toBe("recent_activity");
    expect(urlToState("").query.sort).toBe("recent_activity");
  });

  it("round-trips query fields and omits empty tag filters", () => {
    const state: ViewState = {
      selectedId: "mem-1",
      filtersExpanded: true,
      query: {
        sort: "oldest",
        limit: 25,
        q: "alpha",
        tag: ["sleep", "tool"],
        orphaned: true,
        minImportance: 0.5,
      },
    };

    expect(urlToState(stateToUrl(state))).toEqual({
      ...state,
      filtersExpanded: false,
    });

    const withoutTags = reduce(state, {
      type: "setQuery",
      query: { tag: [] },
    });

    expect(withoutTags.query.tag).toBeUndefined();
    expect(stateToUrl(withoutTags)).not.toContain("tag=");
  });
});
