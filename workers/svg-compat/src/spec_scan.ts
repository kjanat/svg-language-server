/**
 * SVG 2 spec-source scanner.
 *
 * Reads a local checkout of the W3C svgwg repository
 * (`https://github.com/w3c/svgwg`) and extracts machine-readable facts
 * about which elements, attributes, and properties SVG 2 defines,
 * removes, or obsoletes. The output is the authoritative signal the
 * `reconcile_bcd_spec` build check consumes as the THIRD data source
 * alongside BCD flags and snapshot membership.
 *
 * **Parser-first, not hand-curated.** Every fact in the output JSON
 * comes from a structural pattern match against the upstream source
 * files. No manual classification, no prose summarisation. When the
 * spec evolves, re-run the scanner — don't rewrite the output by hand.
 *
 * ## Sources consumed
 *
 * | File | Purpose | Parsing strategy |
 * |---|---|---|
 * | `master/definitions.xml` | Top-level SVG 2 inventory (elements, attrs, properties) | `<element name='…'>` / `<attribute name='…'>` / `<property name='…'>` tag matches |
 * | `master/definitions-filters.xml` | Filter Effects module inventory | same |
 * | `master/definitions-masking.xml` | CSS Masking module inventory | same |
 * | `master/definitions-compositing.xml` | Compositing module inventory | same |
 * | `master/text.html` | Per-property status override notes (removed / obsoleted) | `<h4 id='XxxProperty'>` section + following `<p[ .]*>…has been (removed\|obsoleted) in SVG 2</p>` |
 * | `master/changes.html` | Changelog entries naming removed features | `<li>Removed the <span class='element\|property\|attr-name'>…</span></li>` |
 *
 * The `<element>`, `<attribute>`, and `<property>` tags in the XML
 * files use a custom namespace (`http://mcc.id.au/ns/local`) — we don't
 * do full XML parsing, just pattern-match the tags we care about.
 *
 * ## Known spec quirks the parser handles
 *
 * - `text.html` wraps some status sentences across multiple lines
 *   (`The 'kerning' property has been\n    removed in SVG 2.`) — matched
 *   via a multiline regex rather than a line-by-line scan.
 * - `changes.html` sometimes lists multiple removals per `<li>` (e.g.
 *   "Removed the 'altGlyph', 'altGlyphDef', 'altGlyphItem' and 'glyphRef'
 *   elements.") — we walk every `<span class='…'>` within a Removed-li
 *   rather than assuming one per block.
 * - `definitions.xml` lists `glyph-orientation-horizontal` as a
 *   `<property>` even though the text.html prose says it was removed.
 *   That's a back-compat listing for serialization; the `text.html`
 *   override is authoritative for status. Both are recorded in the
 *   output so consumers can see the disagreement.
 *
 * @module
 */

/** Kind of feature the spec scanner emits facts about. */
export type FeatureKind = "element" | "attribute" | "property";

/** Declared status of a feature per the spec prose. */
export type SpecStatus = "defined" | "removed" | "obsoleted";

/**
 * Provenance for a single scanner fact — file path (relative to
 * `svgwg/master/`), 1-based line number, and the matched text
 * verbatim so future reviewers can audit the classification.
 */
export interface SpecProvenance {
	/** Path relative to `svgwg/master/`, e.g. `"text.html"`. */
	file: string;
	/** 1-based line number of the first line of the match. */
	line: number;
	/** The literal matched text, cleaned of surrounding whitespace only. */
	text: string;
}

/** One scanner fact: a feature with a declared status and where we saw it. */
export interface SpecFact {
	/** Feature name (e.g. `"rect"`, `"xlink:href"`, `"kerning"`). */
	name: string;
	/** What the feature is. */
	kind: FeatureKind;
	/** Spec-asserted status. */
	status: SpecStatus;
	/** Where in the spec this fact came from. */
	provenance: SpecProvenance;
}

/** Top-level scanner report. */
export interface SpecReport {
	/** Schema version for the emitted JSON. Bump on breaking shape changes. */
	schema_version: 1;
	/** Git metadata identifying the exact spec revision scanned. */
	source_pin: {
		repository: "https://github.com/w3c/svgwg";
		commit: string;
		commit_date?: string;
		generated_at: string;
	};
	/** Every `<element>` declared in any `definitions*.xml`. */
	defined_elements: SpecFact[];
	/** Every `<attribute>` declared in any `definitions*.xml`. */
	defined_attributes: SpecFact[];
	/** Every `<property>` declared in any `definitions*.xml`. */
	defined_properties: SpecFact[];
	/** Properties `text.html` explicitly marks "has been removed in SVG 2". */
	removed_properties: SpecFact[];
	/** Properties `text.html` explicitly marks "has been obsoleted ...". */
	obsoleted_properties: SpecFact[];
	/** Features `changes.html` lists under "Removed the ... <span class='...'>". */
	changelog_removals: SpecFact[];
}

interface LineText {
	line: number;
	text: string;
}

/**
 * Regex for `<element name='…'>` / `<attribute name='…'>` /
 * `<property name='…'>` declarations inside `definitions*.xml`.
 *
 * Captures:
 * 1. Tag kind (`element` / `attribute` / `property`)
 * 2. Feature name
 */
const DEFINITION_TAG = /<(element|attribute|property)\s+name=['"]([^'"]+)['"]/g;

/**
 * Regex for the per-property `<h4 id='XxxProperty'>` headings in
 * `text.html`. Captures the property name from the nested span.
 */
const TEXT_HTML_H4_PROPERTY =
	/<h4\s+id=['"]([^'"]*Property)['"][^>]*>\s*The\s*<span[^>]*class=['"]property['"][^>]*>'?([^<']+)'?<\/span>/i;

/**
 * Multiline regex for "has been removed in SVG 2" / "has been obsoleted"
 * sentences inside a `<p>` element. Matches the paragraph body so the
 * scanner can associate a sentence with its enclosing property section.
 *
 * Allows arbitrary whitespace (including newlines) between tokens
 * because the spec sometimes wraps the sentence across lines, e.g.
 * `The <span class='property'>'kerning'</span> property has been\n
 *     removed in SVG 2.`.
 */
const HAS_BEEN_REMOVED = /has\s+been\s+removed\s+in\s+SVG\s*2/i;
const HAS_BEEN_OBSOLETED = /has\s+been\s+obsoleted/i;

/**
 * Regex for a `<li>Removed the ...</li>` entry in `changes.html`. The
 * li can span multiple lines so we match lazily up to the closing tag.
 */
const REMOVED_LI = /<li[^>]*>\s*Removed\s+the\s+([\s\S]*?)<\/li>/gi;

/**
 * Regex for a single `<span class='…'>'name'</span>` inside a Removed
 * changelog entry. The class disambiguates the kind; we recognise
 * `element`, `property`, and `attr-name` — everything else (e.g.
 * `code`, `a href`, informal prose) is out of scope because we can't
 * confidently classify it.
 */
const REMOVED_SPAN = /<span\s+class=['"](element|property|attr-name)['"][^>]*>'?([^<']+?)'?<\/span>/g;

/**
 * Split a file's content into 1-indexed lines paired with their text.
 * Used for provenance line numbers on multiline matches — we locate
 * the match's byte offset, then binary-search the line index.
 */
function indexLines(content: string): {
	lineStarts: number[];
	lines: LineText[];
} {
	const lineStarts: number[] = [0];
	const lines: LineText[] = [];
	let current = "";
	let currentLine = 1;
	for (let i = 0; i < content.length; i++) {
		const ch = content[i];
		if (ch === "\n") {
			lines.push({ line: currentLine, text: current });
			current = "";
			currentLine++;
			lineStarts.push(i + 1);
		} else {
			current += ch;
		}
	}
	if (current.length > 0) {
		lines.push({ line: currentLine, text: current });
	}
	return { lineStarts, lines };
}

function lineAtOffset(lineStarts: number[], offset: number): number {
	// Binary search: last lineStart <= offset
	let lo = 0;
	let hi = lineStarts.length - 1;
	while (lo < hi) {
		const mid = (lo + hi + 1) >>> 1;
		if (lineStarts[mid] <= offset) lo = mid;
		else hi = mid - 1;
	}
	return lo + 1;
}

/**
 * Parse a `definitions*.xml` file for the top-level declared features.
 * Returns facts with `status: "defined"` — the presence in the XML is
 * the signal.
 */
export function parseDefinitionsXml(
	content: string,
	relativePath: string,
): SpecFact[] {
	const { lineStarts } = indexLines(content);
	const facts: SpecFact[] = [];
	DEFINITION_TAG.lastIndex = 0;
	let match: RegExpExecArray | null;
	while ((match = DEFINITION_TAG.exec(content)) !== null) {
		const [fullMatch, tag, name] = match;
		const kind = tag as FeatureKind;
		facts.push({
			name,
			kind,
			status: "defined",
			provenance: {
				file: relativePath,
				line: lineAtOffset(lineStarts, match.index),
				text: fullMatch.trim(),
			},
		});
	}
	return facts;
}

/**
 * Parse `text.html` for per-property status overrides.
 *
 * Walks `<h4 id='XxxProperty'>` sections and captures the first
 * following `<p[^>]*>…</p>` whose body contains "has been removed in
 * SVG 2" or "has been obsoleted". The search window is bounded by the
 * next `<h4>` so we never attribute a sentence to the wrong section.
 */
export function parseTextHtmlOverrides(
	content: string,
	relativePath: string,
): { removed: SpecFact[]; obsoleted: SpecFact[] } {
	const { lineStarts } = indexLines(content);
	const removed: SpecFact[] = [];
	const obsoleted: SpecFact[] = [];

	// Find all h4 property-section boundaries.
	const headings: Array<{ name: string; start: number; headingText: string }> = [];
	const headingRe = new RegExp(TEXT_HTML_H4_PROPERTY.source, "gi");
	let headingMatch: RegExpExecArray | null;
	while ((headingMatch = headingRe.exec(content)) !== null) {
		headings.push({
			name: headingMatch[2],
			start: headingMatch.index,
			headingText: headingMatch[0].trim(),
		});
	}

	// Additionally find any h4 whose *text* contains a property span but
	// whose id didn't match the "Property" suffix pattern — covers
	// variant heading forms. Safe because downstream dedupes by name.
	const genericHeadingRe =
		/<h4\s+[^>]*>\s*The\s*<span[^>]*class=['"]property['"][^>]*>'?([^<']+)'?<\/span>[^<]*<\/h4>/gi;
	let generic: RegExpExecArray | null;
	while ((generic = genericHeadingRe.exec(content)) !== null) {
		if (!headings.some((h) => h.start === generic!.index)) {
			headings.push({
				name: generic[1],
				start: generic.index,
				headingText: generic[0].trim(),
			});
		}
	}

	headings.sort((a, b) => a.start - b.start);

	for (let i = 0; i < headings.length; i++) {
		const heading = headings[i];
		const nextStart = i + 1 < headings.length ? headings[i + 1].start : content.length;
		const sectionBody = content.slice(heading.start, nextStart);

		// Walk `<p[^>]*>...</p>` blocks inside the section. Stop at the
		// first one that contains a status sentence; ignore the rest.
		const paragraphRe = /<p\b[^>]*>([\s\S]*?)<\/p>/gi;
		let p: RegExpExecArray | null;
		while ((p = paragraphRe.exec(sectionBody)) !== null) {
			const body = p[1];
			if (HAS_BEEN_REMOVED.test(body)) {
				const absoluteOffset = heading.start + p.index;
				removed.push({
					name: heading.name,
					kind: "property",
					status: "removed",
					provenance: {
						file: relativePath,
						line: lineAtOffset(lineStarts, absoluteOffset),
						text: stripHtml(body).trim(),
					},
				});
				break;
			}
			if (HAS_BEEN_OBSOLETED.test(body)) {
				const absoluteOffset = heading.start + p.index;
				obsoleted.push({
					name: heading.name,
					kind: "property",
					status: "obsoleted",
					provenance: {
						file: relativePath,
						line: lineAtOffset(lineStarts, absoluteOffset),
						text: stripHtml(body).trim(),
					},
				});
				break;
			}
		}
	}

	return { removed, obsoleted };
}

/**
 * Parse `changes.html` for `<li>Removed the ...</li>` entries.
 *
 * Only matches feature names that are explicitly wrapped in
 * `<span class='element|property|attr-name'>` — anything identified
 * by prose alone is skipped to avoid false positives like
 * "Removed the use element from list of elements that the 'visibility'
 * property directly affects" (which removes a mention, not the element).
 */
export function parseChangesLog(
	content: string,
	relativePath: string,
): SpecFact[] {
	const { lineStarts } = indexLines(content);
	const facts: SpecFact[] = [];

	REMOVED_LI.lastIndex = 0;
	let liMatch: RegExpExecArray | null;
	while ((liMatch = REMOVED_LI.exec(content)) !== null) {
		const [, body] = liMatch;
		const liStartOffset = liMatch.index;
		const liLine = lineAtOffset(lineStarts, liStartOffset);

		// Extract every classed span inside this removal entry.
		const spanRe = new RegExp(REMOVED_SPAN.source, "g");
		let span: RegExpExecArray | null;
		while ((span = spanRe.exec(body)) !== null) {
			const [, spanClass, rawName] = span;
			const kind: FeatureKind = spanClass === "attr-name" ? "attribute" : spanClass as FeatureKind;
			const name = rawName.trim();
			if (name.length === 0) continue;
			facts.push({
				name,
				kind,
				status: "removed",
				provenance: {
					file: relativePath,
					line: liLine,
					text: stripHtml(body).trim().replace(/\s+/g, " "),
				},
			});
		}
	}

	return facts;
}

/** Strip HTML tags for provenance display. Preserves only text nodes. */
function stripHtml(input: string): string {
	return input.replace(/<[^>]+>/g, "").replace(/\s+/g, " ").trim();
}

/**
 * Run the full scanner over a local svgwg checkout.
 *
 * Reads every source file from `${svgwgRoot}/master/` synchronously
 * via Deno's file API. Callers provide the commit hash (typically
 * `git rev-parse HEAD` inside the svgwg repo) so the output is pinned.
 */
export async function scanSvg2Spec(params: {
	svgwgRoot: string;
	commit: string;
	commitDate?: string;
}): Promise<SpecReport> {
	const { svgwgRoot, commit, commitDate } = params;
	const master = `${svgwgRoot.replace(/\/$/, "")}/master`;

	const definedElements: SpecFact[] = [];
	const definedAttributes: SpecFact[] = [];
	const definedProperties: SpecFact[] = [];

	for (
		const xmlFile of [
			"definitions.xml",
			"definitions-filters.xml",
			"definitions-masking.xml",
			"definitions-compositing.xml",
		]
	) {
		const content = await safeRead(`${master}/${xmlFile}`);
		if (content === undefined) continue;
		const facts = parseDefinitionsXml(content, xmlFile);
		for (const fact of facts) {
			if (fact.kind === "element") definedElements.push(fact);
			else if (fact.kind === "attribute") definedAttributes.push(fact);
			else definedProperties.push(fact);
		}
	}

	const textHtml = await safeRead(`${master}/text.html`);
	const overrides = textHtml
		? parseTextHtmlOverrides(textHtml, "text.html")
		: { removed: [], obsoleted: [] };

	const changesHtml = await safeRead(`${master}/changes.html`);
	const changelogRemovals = changesHtml
		? parseChangesLog(changesHtml, "changes.html")
		: [];

	return {
		schema_version: 1,
		source_pin: {
			repository: "https://github.com/w3c/svgwg",
			commit,
			commit_date: commitDate,
			generated_at: new Date().toISOString(),
		},
		defined_elements: dedupe(definedElements),
		defined_attributes: dedupe(definedAttributes),
		defined_properties: dedupe(definedProperties),
		removed_properties: dedupe(overrides.removed),
		obsoleted_properties: dedupe(overrides.obsoleted),
		changelog_removals: dedupe(changelogRemovals),
	};
}

async function safeRead(path: string): Promise<string | undefined> {
	try {
		return await Deno.readTextFile(path);
	} catch (error) {
		if (error instanceof Deno.errors.NotFound) return undefined;
		throw error;
	}
}

/**
 * Deduplicate facts by `(kind, name)` — the same property can be
 * declared in multiple definition files (e.g. a filter primitive's
 * attribute also appearing in the compositing module). First-seen wins,
 * preserving the earliest provenance.
 */
function dedupe(facts: SpecFact[]): SpecFact[] {
	const seen = new Set<string>();
	const out: SpecFact[] = [];
	for (const fact of facts) {
		const key = `${fact.kind}::${fact.name}`;
		if (seen.has(key)) continue;
		seen.add(key);
		out.push(fact);
	}
	return out.sort((a, b) => {
		if (a.kind !== b.kind) return a.kind.localeCompare(b.kind);
		return a.name.localeCompare(b.name);
	});
}
