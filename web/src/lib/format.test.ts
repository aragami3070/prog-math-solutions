import { expect, it } from "vitest";
import { plural } from "./format";

it("picks the Russian plural form", () => {
  const form = (n: number) => plural(n, "источник", "источника", "источников");
  expect([1, 2, 5, 11, 12, 21, 22, 25].map(form)).toEqual(
    ["источник", "источника", "источников", "источников", "источников", "источник", "источника", "источников"],
  );
});
