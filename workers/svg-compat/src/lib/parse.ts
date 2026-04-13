/**
 * Extraction primitives that turn raw upstream JSON (BCD `__compat`
 * + web-features feature map) into typed `CompatEntry` / `Baseline`
 * / `BrowserSupport` objects.
 *
 * Two non-negotiable design rules govern this module:
 *
 *  1. **Never pass raw upstream strings we couldn't interpret
 *     through to consumers.** If we can't parse `≤2021-04-02`,
 *     downstream readers of `/data.json` (including the Rust
 *     crates) shouldn't have to reinvent the parser. The parsed
 *     form is the canonical output.
 *
 *  2. **Never discard anything in the pipeline. Warn loudly on
 *     every unknown.** Replace `return undefined` paths with
 *     degrade-and-preserve. `parseBaselineDate` is a *total*
 *     function for any non-empty input — it always returns a
 *     `BaselineDate` so the original byte-string survives even
 *     when parsing fails. `parseBaseline` similarly never
 *     discards a baseline tier we received.
 *
 * @module
 */

import { isRecord, type JsonRecord } from "../sources.ts";
import type {
	Baseline,
	BaselineDate,
	BrowserFlag,
	BrowserSupport,
	BrowserVersion,
	CompatEntry,
	VersionQualifier,
} from "./types.ts";

const WEB_FEATURE_KIND_FEATURE = "feature";

const loggedWarnings = new Set<string>();

/**
 * Stable string representation of an unknown-typed value, used to
 * key `warnOnce` so identical unknowns don't spam the log.
 */
export function stringifyUnknown(value: unknown): string {
	if (typeof value === "string") return JSON.stringify(value);
	if (typeof value === "number" || typeof value === "boolean") return String(value);
	if (value === null) return "null";
	if (value === undefined) return "undefined";
	try {
		return JSON.stringify(value);
	} catch {
		return String(value);
	}
}

/** Emits `console.warn(message)` exactly once per distinct `key`. */
export function warnOnce(key: string, message: string): void {
	if (loggedWarnings.has(key)) return;
	loggedWarnings.add(key);
	console.warn(message);
}

/** Test helper — clears the warn-dedupe cache between unit tests. */
export function _resetLoggedWarnings(): void {
	loggedWarnings.clear();
}

export function getString(value: unknown): string | undefined {
	return typeof value === "string" ? value : undefined;
}

export function getBoolean(value: unknown): boolean | undefined {
	return typeof value === "boolean" ? value : undefined;
}

export function getStringArray(value: unknown): string[] | undefined {
	if (!Array.isArray(value)) return undefined;
	const strings = value.filter((entry): entry is string => typeof entry === "string");
	return strings.length === value.length ? strings : undefined;
}

export function getRecord(value: unknown): JsonRecord | undefined {
	return isRecord(value) ? value : undefined;
}

export function getRecordProperty(record: JsonRecord, key: string): JsonRecord | undefined {
	return getRecord(record[key]);
}

export function getCompat(node: JsonRecord): JsonRecord | undefined {
	return getRecordProperty(node, "__compat");
}

/**
 * Lookup table for known web-features baseline date prefixes.
 *
 * Currently the dataset only ships `≤` (14 distinct date strings as of v3.23.0),
 * but the upstream schema declares the field as plain `string` with no pattern,
 * so future versions could ship `≥` / `~` / etc.
 *
 * Extend this table when that happens — `parseBaselineDate` will warn on any
 * unknown prefix until it's added here.
 */
const KNOWN_DATE_PREFIXES: Record<string, BaselineDate["qualifier"]> = {
	"≤": "before",
	"<": "before",
	"<=": "before",
	"≥": "after",
	">": "after",
	">=": "after",
	"~": "approximately",
};

/**
 * Parses a web-features baseline date string into our typed form.
 *
 * Total function: given any non-empty `raw`, **always** returns a
 * `BaselineDate`. The caller can inspect whether `date` was
 * extractable.
 *
 * ```
 *   "2021-04-02"   → { raw: "2021-04-02", date: "2021-04-02" }
 *   "≤2021-04-02"  → { raw: "≤2021-04-02", date: "2021-04-02", qualifier: "before" }
 *   "~2024-01-01"  → { raw: "~2024-01-01", date: "2024-01-01", qualifier: "approximately" }
 *   "garbage-2021" → { raw: "garbage-2021" }   + warnOnce
 *   "2099"         → { raw: "2099" }           + warnOnce
 *   "%2024-01-01"  → { raw: "%2024-01-01", date: "2024-01-01", qualifier: "approximately" } + warnOnce
 * ```
 *
 * Unknown prefixes still produce a parsed `date` (the prefix is
 * stripped) and `qualifier: "approximately"`, but also fire a
 * `warnOnce` so an operator can extend `KNOWN_DATE_PREFIXES`.
 *
 * Returns `undefined` only when `raw` itself is `undefined` or
 * empty — that's the "we never had any data" case, not a discard.
 */
export function parseBaselineDate(
	raw: string | undefined,
	compatKey: string,
): BaselineDate | undefined {
	if (!raw) return undefined;
	const match = raw.match(/^(\D*)(\d{4}-\d{2}-\d{2})/);
	if (!match) {
		warnOnce(
			`wf-date-unparseable:${raw}`,
			`svg-compat: could not extract YYYY-MM-DD from baseline date ${
				stringifyUnknown(raw)
			} for "${compatKey}". Preserving as raw.`,
		);
		return { raw };
	}
	const [, prefix, isoDate] = match;
	if (Number.isNaN(Date.parse(isoDate))) {
		warnOnce(
			`wf-date-invalid-iso:${isoDate}`,
			`svg-compat: extracted "${isoDate}" from ${
				stringifyUnknown(raw)
			} but it is not a valid date for "${compatKey}". Preserving as raw.`,
		);
		return { raw };
	}
	if (prefix.length === 0) return { raw, date: isoDate };
	const known = KNOWN_DATE_PREFIXES[prefix];
	if (known) return { raw, date: isoDate, qualifier: known };
	warnOnce(
		`wf-date-prefix:${prefix}`,
		`svg-compat: unrecognised baseline date prefix ${stringifyUnknown(prefix)} (in ${
			stringifyUnknown(raw)
		}) — treating as "approximately". Add it to KNOWN_DATE_PREFIXES if it should map to "before" or "after".`,
	);
	return { raw, date: isoDate, qualifier: "approximately" };
}

/**
 * Year extraction. Only useful once we know the date parsed
 * cleanly — if `parsed.date` is undefined this returns undefined
 * and the caller leaves `Baseline.since` unset.
 */
export function yearOfBaselineDate(parsed: BaselineDate): number | undefined {
	if (!parsed.date) return undefined;
	const ts = Date.parse(parsed.date);
	if (Number.isNaN(ts)) return undefined;
	return new Date(ts).getUTCFullYear();
}

/**
 * Parses the `status` block of a web-features feature into our
 * `Baseline` shape. Never discards data we received: an unknown
 * baseline value becomes `status: "limited"` with the original
 * stashed in `raw_status`, and a date that can't be parsed still
 * yields a `BaselineDate` with `raw` set.
 *
 * Returns `undefined` only when there is genuinely no upstream
 * baseline information to emit (no `baseline` field and no dates).
 */
export function parseBaseline(
	status: JsonRecord,
	compatKey: string,
): Baseline | undefined {
	const baselineValue = status.baseline;
	const low = parseBaselineDate(getString(status.baseline_low_date), compatKey);
	const high = parseBaselineDate(getString(status.baseline_high_date), compatKey);

	// Nothing at all upstream → nothing to emit. NOT a discard:
	// we never had any baseline data for this entry to begin with.
	if (baselineValue === undefined && !low && !high) return undefined;

	if (baselineValue === false) {
		// "limited" tier normally has no dates; preserve any that
		// upstream did include rather than dropping them.
		return { status: "limited", low_date: low, high_date: high };
	}

	if (baselineValue === "high") {
		return {
			status: "widely",
			since: high ? yearOfBaselineDate(high) : undefined,
			since_qualifier: high?.qualifier,
			low_date: low,
			high_date: high,
		};
	}

	if (baselineValue === "low") {
		return {
			status: "newly",
			since: low ? yearOfBaselineDate(low) : undefined,
			since_qualifier: low?.qualifier,
			low_date: low,
		};
	}

	// Unknown baseline value — warn AND preserve. Fall back to
	// "limited" (safest visual default) but stash the original
	// in `raw_status` so it isn't lost.
	warnOnce(
		`wf-baseline:${stringifyUnknown(baselineValue)}`,
		`svg-compat: unsupported baseline value ${
			stringifyUnknown(baselineValue)
		} for "${compatKey}". Falling back to "limited" and preserving as raw_status.`,
	);
	return {
		status: "limited",
		raw_status: stringifyUnknown(baselineValue),
		low_date: low,
		high_date: high,
	};
}

/** Resolves baseline from web-features using `web-features:` tags in BCD `__compat.tags`. */
export function extractBaseline(
	compat: JsonRecord,
	featureMap: JsonRecord,
	compatKey: string,
): Baseline | undefined {
	const tags = getStringArray(compat.tags);
	if (!tags) return undefined;

	const featureTag = tags.find((tag) => tag.startsWith("web-features:"));
	if (!featureTag) return undefined;

	const featureId = featureTag.slice("web-features:".length);
	const feature = getRecordProperty(featureMap, featureId);
	if (!feature) return undefined;
	const featureKind = getString(feature.kind);
	if (featureKind !== WEB_FEATURE_KIND_FEATURE) {
		warnOnce(
			`wf-kind:${featureKind ?? "<missing>"}`,
			`svg-compat: unsupported web-features kind ${stringifyUnknown(featureKind)} for "${featureId}".`,
		);
		return undefined;
	}

	const status = getRecordProperty(feature, "status");
	if (!status) return undefined;

	const byCompatKey = getRecordProperty(status, "by_compat_key");
	const overrideStatus = byCompatKey ? getRecordProperty(byCompatKey, compatKey) : undefined;
	if (overrideStatus) return parseBaseline(overrideStatus, compatKey);

	return parseBaseline(status, compatKey);
}

/**
 * Lookup table for known BCD version-string prefixes. Same set as
 * `KNOWN_DATE_PREFIXES` — version numbers carry the same "at or
 * before / at or after" semantics as baseline dates.
 */
const KNOWN_VERSION_PREFIXES: Record<string, VersionQualifier> = {
	"≤": "before",
	"<": "before",
	"<=": "before",
	"≥": "after",
	">": "after",
	">=": "after",
	"~": "approximately",
};

interface ParsedVersionString {
	version: string;
	qualifier?: VersionQualifier;
}

/**
 * Splits a BCD version string into a clean version + qualifier.
 * Mirrors `parseBaselineDate` on version numbers rather than dates.
 * Returns `undefined` only when `raw` is empty or not a string.
 *
 * Examples:
 *   "50"   → { version: "50" }
 *   "≤50"  → { version: "50", qualifier: "before" }
 *   "<=50" → { version: "50", qualifier: "before" }
 *   "~50"  → { version: "50", qualifier: "approximately" }
 *   "%50"  → { version: "50", qualifier: "approximately" } + warnOnce
 */
function parseBrowserVersionString(
	raw: string,
	compatKey: string,
): ParsedVersionString | undefined {
	if (raw.length === 0) return undefined;
	const match = raw.match(/^([^0-9A-Za-z]*)(.+)$/);
	if (!match) return undefined;
	const [, prefix, body] = match;
	if (body.length === 0) return undefined;
	if (prefix.length === 0) return { version: body };
	const known = KNOWN_VERSION_PREFIXES[prefix];
	if (known) return { version: body, qualifier: known };
	warnOnce(
		`wf-version-prefix:${prefix}`,
		`svg-compat: unrecognised version prefix ${stringifyUnknown(prefix)} (in ${
			stringifyUnknown(raw)
		}) for "${compatKey}" — treating as "approximately". Add it to KNOWN_VERSION_PREFIXES if it should map to "before" or "after".`,
	);
	return { version: body, qualifier: "approximately" };
}

function parseBrowserNotes(value: unknown): string[] | undefined {
	if (typeof value === "string") return [value];
	if (!Array.isArray(value)) return undefined;
	const notes = value.filter((entry): entry is string => typeof entry === "string");
	return notes.length > 0 ? notes : undefined;
}

function parseBrowserFlags(
	value: unknown,
	compatKey: string,
	browser: string,
): BrowserFlag[] | undefined {
	if (!Array.isArray(value)) return undefined;
	const flags: BrowserFlag[] = [];
	for (const entry of value) {
		if (!isRecord(entry)) continue;
		const type = getString(entry.type);
		const name = getString(entry.name);
		if (type === undefined || name === undefined) {
			warnOnce(
				`wf-flag-shape:${stringifyUnknown(entry)}`,
				`svg-compat: unrecognised flag shape ${stringifyUnknown(entry)} for "${compatKey}" / ${browser}. Skipping.`,
			);
			continue;
		}
		const flag: BrowserFlag = { type, name };
		const valueToSet = getString(entry.value_to_set);
		if (valueToSet !== undefined) flag.value_to_set = valueToSet;
		flags.push(flag);
	}
	return flags.length > 0 ? flags : undefined;
}

/**
 * Parses a single browser's `support` entry from a BCD compat block
 * into our typed `BrowserVersion` form.
 *
 * `support[browser]` can be:
 * - a single statement object (most common),
 * - an array of statements (BCD convention: most-recent first),
 * - absent entirely — returns `undefined`.
 *
 * When the upstream shape is unexpected, we warn and return
 * `undefined` rather than inventing data. When the upstream shape
 * is recognised, we ALWAYS return a `BrowserVersion` — no silent
 * discard, even for `version_added: false`.
 */
export function parseBrowserVersion(
	value: unknown,
	browser: string,
	compatKey: string,
): BrowserVersion | undefined {
	if (value === undefined) return undefined;
	const stmt = isRecord(value)
		? value
		: Array.isArray(value) && value.length > 0 && isRecord(value[0])
		? value[0]
		: undefined;
	if (!stmt) {
		warnOnce(
			`wf-browser-shape:${browser}`,
			`svg-compat: unrecognised support statement shape ${
				stringifyUnknown(value)
			} for "${compatKey}" / ${browser}. Skipping.`,
		);
		return undefined;
	}

	const rawAdded = stmt.version_added;
	let raw_value_added: BrowserVersion["raw_value_added"];
	if (
		typeof rawAdded === "string"
		|| typeof rawAdded === "boolean"
		|| rawAdded === null
	) {
		raw_value_added = rawAdded;
	} else {
		warnOnce(
			`wf-version-added-type:${typeof rawAdded}`,
			`svg-compat: unexpected version_added type ${
				stringifyUnknown(rawAdded)
			} for "${compatKey}" / ${browser}. Coercing to null.`,
		);
		raw_value_added = null;
	}

	const result: BrowserVersion = { raw_value_added };

	if (typeof raw_value_added === "string") {
		const parsed = parseBrowserVersionString(raw_value_added, compatKey);
		if (parsed) {
			result.version_added = parsed.version;
			if (parsed.qualifier !== undefined) result.version_qualifier = parsed.qualifier;
		}
	} else if (raw_value_added === false) {
		result.supported = false;
	} else if (raw_value_added === true) {
		result.supported = true;
	}

	const rawRemoved = getString(stmt.version_removed);
	if (rawRemoved !== undefined) {
		const parsedRemoved = parseBrowserVersionString(rawRemoved, compatKey);
		if (parsedRemoved) {
			result.version_removed = parsedRemoved.version;
			if (parsedRemoved.qualifier !== undefined) {
				result.version_removed_qualifier = parsedRemoved.qualifier;
			}
		}
	}

	if (stmt.partial_implementation === true) result.partial_implementation = true;
	const prefix = getString(stmt.prefix);
	if (prefix !== undefined) result.prefix = prefix;
	const altName = getString(stmt.alternative_name);
	if (altName !== undefined) result.alternative_name = altName;
	const flags = parseBrowserFlags(stmt.flags, compatKey, browser);
	if (flags !== undefined) result.flags = flags;
	const notes = parseBrowserNotes(stmt.notes);
	if (notes !== undefined) result.notes = notes;

	return result;
}

export function extractBrowserSupport(
	compat: JsonRecord,
	compatKey: string,
): BrowserSupport | undefined {
	const support = getRecordProperty(compat, "support");
	if (!support) return undefined;

	const chrome = parseBrowserVersion(support.chrome, "chrome", compatKey);
	const edge = parseBrowserVersion(support.edge, "edge", compatKey);
	const firefox = parseBrowserVersion(support.firefox, "firefox", compatKey);
	const safari = parseBrowserVersion(support.safari, "safari", compatKey);

	if (
		chrome === undefined
		&& edge === undefined
		&& firefox === undefined
		&& safari === undefined
	) {
		return undefined;
	}

	const result: BrowserSupport = {};
	if (chrome !== undefined) result.chrome = chrome;
	if (edge !== undefined) result.edge = edge;
	if (firefox !== undefined) result.firefox = firefox;
	if (safari !== undefined) result.safari = safari;
	return result;
}

export function extractSpecUrls(compat: JsonRecord): string[] {
	const url = compat.spec_url;
	if (typeof url === "string") return [url];
	if (!Array.isArray(url)) return [];
	return url.filter((entry): entry is string => typeof entry === "string");
}

/** Builds a {@linkcode CompatEntry} from a BCD `__compat` node and web-features lookup. */
export function makeCompatEntry(
	compat: JsonRecord,
	featureMap: JsonRecord,
	compatKey: string,
): CompatEntry {
	const status = getRecordProperty(compat, "status");
	return {
		description: getString(compat.description),
		mdn_url: getString(compat.mdn_url),
		deprecated: getBoolean(status?.deprecated) ?? false,
		experimental: getBoolean(status?.experimental) ?? false,
		standard_track: getBoolean(status?.standard_track) ?? true,
		spec_url: extractSpecUrls(compat),
		baseline: extractBaseline(compat, featureMap, compatKey),
		browser_support: extractBrowserSupport(compat, compatKey),
	};
}
