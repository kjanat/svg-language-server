/**
 * Public type definitions for the SVG compat output.
 *
 * Pure type module — no runtime, no imports beyond `sources.ts` for
 * the upstream-source descriptor types. Both the HTTP server in
 * `main.ts` and the CLI in `cli.ts` consume this module via
 * `lib/mod.ts`, so there is exactly one source of truth for the
 * shape of `/data.json`.
 *
 * @module
 */

import type { SourceInfo, SvgCompatSources } from "../sources.ts";

export type { SourceInfo, SvgCompatSources };

/**
 * A baseline date as we received it from web-features, plus the
 * parsed-out clean form when extractable.
 *
 * `raw` is **always** present, even when we successfully parsed
 * `date` and `qualifier` — that way no upstream byte is ever
 * silently lost. Web-features uses prefixes like `≤2021-04-02`
 * for "at or before this date", `≥` / `~` etc. for other forms
 * of uncertainty; the upstream schema declares the field as plain
 * `string` so future versions can ship any prefix.
 *
 * If the parser could not extract a clean `YYYY-MM-DD` from `raw`,
 * only `raw` is set and a `warnOnce` fires so the unknown shape
 * is visible in worker logs.
 */
export interface BaselineDate {
	/** Original upstream value, byte-for-byte. Always present. */
	raw: string;
	/** ISO `YYYY-MM-DD` extracted from `raw`. Absent if unparseable. */
	date?: string;
	/**
	 * Set when `raw` carried a qualifier prefix:
	 *
	 * - `"before"`        — `≤` / `<` / `<=`
	 * - `"after"`         — `≥` / `>` / `>=`
	 * - `"approximately"` — `~`, OR any unknown prefix that we
	 *   recognised as "non-empty but not in our known set". Unknown
	 *   prefixes also trigger a one-time warning so future schema
	 *   changes can't slip through unnoticed.
	 */
	qualifier?: "before" | "after" | "approximately";
}

/** Web-platform baseline status resolved from the `web-features` dataset. */
export interface Baseline {
	/** Baseline tier: widely available, newly available, or limited support. */
	status: "widely" | "newly" | "limited";
	/**
	 * Set when the upstream `baseline` value was something other
	 * than `false` / `"high"` / `"low"`. The original value is
	 * preserved here verbatim so it is never lost; `status` falls
	 * back to `"limited"` (safest visual default) and a `warnOnce`
	 * is fired so an operator can investigate.
	 */
	raw_status?: string;
	/**
	 * Year derived from `high_date.date` when status is `"widely"`,
	 * from `low_date.date` when `"newly"`. Convenience field for the
	 * baseline badge; downstream consumers can recompute it from the
	 * date sub-objects if they need finer precision.
	 */
	since?: number;
	/**
	 * Mirror of the qualifier on whichever date `since` was derived
	 * from, so the badge can render `≤2021` without reaching into
	 * the date sub-object.
	 */
	since_qualifier?: BaselineDate["qualifier"];
	/** When the feature first reached baseline (low tier). */
	low_date?: BaselineDate;
	/** When the feature reached baseline high tier. */
	high_date?: BaselineDate;
}

/** Minimum browser versions that support a feature, from BCD `support` block. */
export interface BrowserSupport {
	/** Minimum Chrome desktop version. */
	chrome?: string;
	/** Minimum Edge version. */
	edge?: string;
	/** Minimum Firefox desktop version. */
	firefox?: string;
	/** Minimum Safari desktop version. */
	safari?: string;
}

/** Processed compatibility entry for an SVG element or attribute. */
export interface CompatEntry {
	/** Human-readable feature description from BCD. */
	description?: string;
	/** MDN documentation URL. */
	mdn_url?: string;
	/** Whether the feature is deprecated. */
	deprecated: boolean;
	/** Whether the feature is experimental (single-implementer). */
	experimental: boolean;
	/** Whether the feature is on a standards track. */
	standard_track: boolean;
	/** Specification URLs from BCD. */
	spec_url: string[];
	/** Baseline status from web-features. */
	baseline?: Baseline;
	/** Minimum browser versions from BCD. */
	browser_support?: BrowserSupport;
}

/** Compat entry for an attribute, with the list of elements it applies to (`["*"]` = global). */
export interface AttributeEntry extends CompatEntry {
	/** Element names this attribute applies to. `["*"]` means global. */
	elements: string[];
}

/** Top-level JSON response shape served at `/data.json`. */
export interface SvgCompatOutput {
	/** ISO timestamp of when this output was generated. */
	generated_at: string;
	/** Upstream package versions used to build this response. */
	sources: SvgCompatSources;
	/** SVG elements keyed by tag name. */
	elements: Record<string, CompatEntry>;
	/** SVG attributes keyed by attribute name (xlink uses colon notation). */
	attributes: Record<string, AttributeEntry>;
}

/** Internal snapshot before timestamp is added. Used as the cache value. */
export interface SvgCompatSnapshot {
	/** Resolved upstream package versions. */
	sources: SvgCompatSources;
	/** Processed SVG elements. */
	elements: Record<string, CompatEntry>;
	/** Processed SVG attributes. */
	attributes: Record<string, AttributeEntry>;
}
