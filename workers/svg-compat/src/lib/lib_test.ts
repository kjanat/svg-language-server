/**
 * Pure unit tests for the lib. No HTTP, no live network — fast and
 * deterministic. Exercises the parse layer directly with synthetic
 * inputs so each rule is covered without depending on whatever
 * shape web-features happens to ship today.
 *
 * @module
 */

import { assertEquals, assertExists } from "@std/assert";
import { _resetLoggedWarnings, parseBaseline, parseBaselineDate } from "./parse.ts";
import type { BaselineDate } from "./types.ts";

Deno.test("parseBaselineDate maps known qualifier prefixes", () => {
	_resetLoggedWarnings();
	const cases: Array<[string, BaselineDate["qualifier"], string]> = [
		["≤2021-04-02", "before", "2021-04-02"],
		["<2020-01-01", "before", "2020-01-01"],
		["<=2020-01-01", "before", "2020-01-01"],
		["≥2024-06-01", "after", "2024-06-01"],
		[">2024-06-01", "after", "2024-06-01"],
		[">=2024-06-01", "after", "2024-06-01"],
		["~2023-08-15", "approximately", "2023-08-15"],
		["2022-12-31", undefined, "2022-12-31"],
	];
	for (const [raw, qualifier, date] of cases) {
		const got = parseBaselineDate(raw, "test.fixture");
		assertExists(got, `expected BaselineDate for ${raw}`);
		assertEquals(got?.raw, raw);
		assertEquals(got?.date, date);
		assertEquals(got?.qualifier, qualifier);
	}
});

Deno.test("parseBaselineDate preserves raw on completely unparseable input", () => {
	_resetLoggedWarnings();
	const got = parseBaselineDate("garbage", "test.fixture");
	assertExists(got);
	assertEquals(got?.raw, "garbage");
	assertEquals(got?.date, undefined);
	assertEquals(got?.qualifier, undefined);
});

Deno.test("parseBaselineDate maps unknown prefix to approximately", () => {
	_resetLoggedWarnings();
	const got = parseBaselineDate("%2024-01-01", "test.fixture");
	assertExists(got);
	assertEquals(got?.raw, "%2024-01-01");
	assertEquals(got?.date, "2024-01-01");
	assertEquals(got?.qualifier, "approximately");
});

Deno.test("parseBaselineDate returns undefined for empty/undefined input only", () => {
	_resetLoggedWarnings();
	assertEquals(parseBaselineDate(undefined, "test.fixture"), undefined);
	assertEquals(parseBaselineDate("", "test.fixture"), undefined);
});

Deno.test("parseBaseline preserves raw on unparseable date but baseline tier is known", () => {
	_resetLoggedWarnings();
	const got = parseBaseline(
		{
			baseline: "high",
			baseline_high_date: "garbage",
			baseline_low_date: "garbage",
		},
		"test.fixture",
	);
	assertExists(got);
	assertEquals(got?.status, "widely");
	assertEquals(got?.high_date?.raw, "garbage");
	assertEquals(got?.high_date?.date, undefined);
	// since is undefined because no extractable date; tier still emitted.
	assertEquals(got?.since, undefined);
});

Deno.test("parseBaseline never discards on unknown baseline value", () => {
	_resetLoggedWarnings();
	const got = parseBaseline({ baseline: "experimental" }, "test.fixture");
	assertExists(got);
	assertEquals(got?.status, "limited");
	assertEquals(got?.raw_status, "\"experimental\"");
});

Deno.test("parseBaseline maps known prefix end-to-end on real-world feGaussianBlur shape", () => {
	_resetLoggedWarnings();
	// Mirror the exact shape we get from web-features 3.23.0
	// for `svg.elements.feGaussianBlur` via by_compat_key.
	const got = parseBaseline(
		{
			baseline: "high",
			baseline_high_date: "≤2021-04-02",
			baseline_low_date: "≤2018-10-02",
		},
		"svg.elements.feGaussianBlur",
	);
	assertExists(got);
	assertEquals(got?.status, "widely");
	assertEquals(got?.since, 2021);
	assertEquals(got?.since_qualifier, "before");
	assertEquals(got?.high_date?.raw, "≤2021-04-02");
	assertEquals(got?.high_date?.date, "2021-04-02");
	assertEquals(got?.high_date?.qualifier, "before");
	assertEquals(got?.low_date?.raw, "≤2018-10-02");
	assertEquals(got?.low_date?.date, "2018-10-02");
	assertEquals(got?.low_date?.qualifier, "before");
});

Deno.test("parseBaseline returns limited with preserved dates when baseline === false", () => {
	_resetLoggedWarnings();
	const got = parseBaseline(
		{
			baseline: false,
			baseline_low_date: "2024-01-01",
		},
		"test.fixture",
	);
	assertExists(got);
	assertEquals(got?.status, "limited");
	assertEquals(got?.low_date?.date, "2024-01-01");
});

Deno.test("parseBaseline returns undefined only when there is no upstream data at all", () => {
	_resetLoggedWarnings();
	assertEquals(parseBaseline({}, "test.fixture"), undefined);
});
