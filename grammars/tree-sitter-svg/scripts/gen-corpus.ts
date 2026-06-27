#!/usr/bin/env node
/**
 * Catalog-seeded corpus generator.
 *
 * Emits one minimal, valid corpus case per SVG element defined in the catalog,
 * exercising the typed attribute buckets the element actually carries. The
 * expected syntax tree for every case is CAPTURED from the built grammar
 * (`tree-sitter parse --no-ranges`) rather than hand-authored, so the generated
 * file always reflects what the parser really produces. A case is only kept if
 * its snippet parses cleanly (no ERROR / MISSING); attributes that would push a
 * snippet into recovery are dropped and the element falls back to its bare form.
 *
 * Sources of truth:
 *   - crates/svg-data/data/catalog.core.json        (elements, attrs, enums)
 *   - crates/svg-data/data/catalog.tree-sitter.json (attribute -> bucket map)
 *
 * Output (deterministic, regenerable):
 *   - test/corpus/generated_catalog_elements.txt
 *
 * Modes:
 *   (default)  write the corpus file
 *   --check    regenerate in memory and fail if the on-disk file is stale
 */

import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { argv } from 'node:process';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const grammarRoot = resolve(scriptDir, '..');
const repoRoot = resolve(grammarRoot, '../..');
const corePath = resolve(repoRoot, 'crates/svg-data/data/catalog.core.json');
const treeSitterPath = resolve(repoRoot, 'crates/svg-data/data/catalog.tree-sitter.json');
const outPath = resolve(grammarRoot, 'test/corpus/generated_catalog_elements.txt');

const GENERATED_HEADER = [
	'; GENERATED FILE - DO NOT EDIT BY HAND.',
	'; Source: scripts/gen-corpus.ts (catalog-seeded corpus generator).',
	'; Regenerate: `bun run gen:corpus`  Verify: `bun run gen:corpus:check`.',
	'; Expected trees are captured from the built grammar, not hand-authored.',
].join('\n');

const DIVIDER = '='.repeat(40);

/** A keyword bucket attribute's value space, taken from the core catalog. */
type AttributeValues =
	| { readonly kind: 'enum'; readonly values: readonly string[] }
	| { readonly kind: 'transform'; readonly functions: readonly string[] }
	| { readonly kind: 'color' }
	| { readonly kind: 'length' }
	| { readonly kind: 'url' }
	| { readonly kind: 'boolean' }
	| { readonly kind: 'token_list' }
	| { readonly kind: 'comma_token_list' }
	| { readonly kind: 'url_token_list' }
	| { readonly kind: 'language_tag' }
	| { readonly kind: 'integer' }
	| { readonly kind: 'media_type' }
	| { readonly kind: 'media_query_list' }
	| { readonly kind: 'css_declaration_list' }
	| { readonly kind: 'id' }
	| { readonly kind: 'referrer_policy' }
	| { readonly kind: 'suggested_file_name' }
	| { readonly kind: 'path_data' }
	| { readonly kind: 'semicolon_number_list' }
	| { readonly kind: 'coordinate_pair' }
	| { readonly kind: 'coordinate_pair_list' }
	| { readonly kind: 'number_or_percentage' }
	| { readonly kind: 'free_text' }
	| { readonly kind: 'css_grammar'; readonly grammar: string; readonly graph: CssGrammarGraph };

type CssGrammarGraph = {
	readonly nodes?: readonly { readonly kind?: string; readonly text?: string }[];
};

type ElementValues = {
	readonly element: string;
	readonly values: AttributeValues;
};

type CoreAttribute = {
	readonly name: string;
	readonly values?: AttributeValues;
	readonly element_values?: readonly ElementValues[];
};

type CoreElement = {
	readonly name: string;
	readonly attrs?: readonly string[];
};

type CoreCatalog = {
	readonly elements: readonly CoreElement[];
	readonly attributes: readonly CoreAttribute[];
};

type TreeSitterCatalog = {
	readonly attribute_buckets: Record<string, readonly string[]>;
};

/**
 * Canonical, bucket-level sample values. Keyed by BUCKET, never by attribute
 * name, so the routing stays category-driven and free of per-name allowlists.
 * The `keyword` bucket has no single valid value (each attribute owns its enum),
 * so it is resolved from the core catalog per attribute instead.
 */
const BUCKET_SAMPLE: Readonly<Record<string, string>> = {
	color: 'red',
	length: '5',
	length_list: '1 2 3',
	length_list_or_none: '4 2',
	number: '0.5',
	number_optional_number: '2 3',
	number_list: '0 1 0 1 0',
	number_or_percentage: '0.5',
	coordinate_pair_list: '0,0 1,1',
	path_data: 'M0 0 L1 1',
	view_box: '0 0 10 10',
	functional_iri: 'url(#f)',
	css_text: 'sans-serif',
};

const SYNTAX_FIRST_BUCKETS = new Set([
	'coordinate_pair_list',
	'path_data',
	'view_box',
	'functional_iri',
]);

function displayPath(path: string): string {
	const rel = relative(repoRoot, path);
	return rel === '' || rel.startsWith('..') ? path : rel;
}

function readJson<T>(path: string): T {
	return JSON.parse(readFileSync(path, 'utf8')) as T;
}

/**
 * Build a name -> bucket index from the tree-sitter catalog. An attribute name
 * appears in exactly one bucket; the first match wins if the catalog ever lists
 * a name twice (it should not).
 */
function buildBucketIndex(catalog: TreeSitterCatalog): Map<string, string> {
	const index = new Map<string, string>();
	for (const [bucket, names] of Object.entries(catalog.attribute_buckets)) {
		for (const name of names) {
			if (!index.has(name)) {
				index.set(name, bucket);
			}
		}
	}
	return index;
}

/**
 * Resolve a deterministic, valid sample value for a keyword-bucket attribute.
 * Enums use their first (alphabetically stable) member. Free-text and
 * css_grammar keyword spaces have no guaranteed literal we can synthesise here,
 * so they yield `null` and the attribute is skipped for that element (the
 * capture step would otherwise have to guess).
 */
function keywordSample(attr: CoreAttribute | undefined): string | null {
	const values = attr?.values;
	if (values?.kind === 'enum' && values.values.length > 0) {
		return [...values.values].sort()[0] ?? null;
	}
	return null;
}

function valuesForElement(attr: CoreAttribute | undefined, elementName: string): AttributeValues | undefined {
	return attr?.element_values?.find((elementValues) => elementValues.element === elementName)?.values
		?? attr?.values;
}

function firstKeyword(values: AttributeValues): string | null {
	if (values.kind === 'enum' && values.values.length > 0) {
		return [...values.values].sort()[0] ?? null;
	}
	if (values.kind !== 'css_grammar') {
		return null;
	}
	const keywords = (values.graph.nodes ?? [])
		.filter((node) => node.kind === 'keyword' && node.text !== undefined)
		.map((node) => node.text ?? '')
		.sort();
	return keywords[0] ?? null;
}

function cssGrammarSample(values: Extract<AttributeValues, { readonly kind: 'css_grammar' }>): string | null {
	const grammar = values.grammar.toLowerCase();
	const keyword = firstKeyword(values);
	if (keyword !== null) {
		return keyword;
	}
	if (grammar.includes('<number-optional-number>')) {
		return '2 3';
	}
	if (grammar.includes('<number>') && (grammar.includes('+') || grammar.includes('#'))) {
		return '0 1 0';
	}
	if (grammar.includes('<length') && (grammar.includes('+') || grammar.includes('#'))) {
		return '1 2 3';
	}
	if (grammar.includes('<number>')) {
		return '0.5';
	}
	if (grammar.includes('<integer>')) {
		return '1';
	}
	if (grammar.includes('<length') || grammar.includes('<angle>')) {
		return '5';
	}
	if (grammar.includes('<url') || grammar.includes('<iri')) {
		return 'url(#f)';
	}
	if (grammar.includes('<color') || grammar.includes('<paint')) {
		return 'red';
	}
	return null;
}

function semanticSample(values: AttributeValues | undefined): string | null {
	if (values === undefined) {
		return null;
	}
	switch (values.kind) {
		case 'enum':
			return firstKeyword(values);
		case 'transform':
			return 'translate(1 2)';
		case 'color':
			return 'red';
		case 'length':
			return '5';
		case 'url':
			return '#target';
		case 'boolean':
			return 'true';
		case 'token_list':
			return 'alpha beta';
		case 'comma_token_list':
			return 'alpha,beta';
		case 'url_token_list':
			return '#a #b';
		case 'language_tag':
			return 'en';
		case 'integer':
			return '1';
		case 'media_type':
			return 'text/css';
		case 'media_query_list':
			return 'screen';
		case 'css_declaration_list':
			return 'fill: red';
		case 'id':
			return 'sample-id';
		case 'referrer_policy':
			return 'no-referrer';
		case 'suggested_file_name':
			return 'example.svg';
		case 'path_data':
			return 'M0 0 L1 1';
		case 'semicolon_number_list':
			return '0; 0.5; 1';
		case 'coordinate_pair':
			return '0,0';
		case 'coordinate_pair_list':
			return '0,0; 1,1';
		case 'number_or_percentage':
			return '0.5';
		case 'css_grammar':
			return cssGrammarSample(values);
		case 'free_text':
			return 'text';
	}
}

function attributeSemanticSample(
	attributeName: string,
	elementName: string,
	values: AttributeValues | undefined,
): string | null {
	if (attributeName === 'type' && values?.kind === 'media_type') {
		if (elementName === 'script') {
			return 'application/ecmascript';
		}
		if (elementName === 'style') {
			return 'text/css';
		}
		if (elementName === 'a') {
			return 'text/html';
		}
	}
	return semanticSample(values);
}

/**
 * Pick one representative attribute per distinct bucket the element carries.
 * Deterministic: buckets are visited in catalog-key order, and within a bucket
 * the alphabetically-first attribute name on the element wins.
 */
function selectAttributes(
	element: CoreElement,
	bucketIndex: Map<string, string>,
	coreByName: Map<string, CoreAttribute>,
): { name: string; value: string }[] {
	const perBucket = new Map<string, { name: string; value: string }>();
	for (const name of [...(element.attrs ?? [])].sort()) {
		const bucket = bucketIndex.get(name);
		if (bucket === undefined || perBucket.has(bucket)) {
			continue;
		}
		const attr = coreByName.get(name);
		let value: string | null;
		if (bucket === 'keyword') {
			value = keywordSample(attr);
		} else if (SYNTAX_FIRST_BUCKETS.has(bucket)) {
			value = BUCKET_SAMPLE[bucket]
				?? attributeSemanticSample(name, element.name, valuesForElement(attr, element.name));
		} else {
			value = attributeSemanticSample(name, element.name, valuesForElement(attr, element.name))
				?? BUCKET_SAMPLE[bucket]
				?? null;
		}
		if (value !== null) {
			perBucket.set(bucket, { name, value });
		}
	}

	const chosen: { name: string; value: string }[] = [];
	for (const attr of perBucket.values()) {
		chosen.push(attr);
	}
	chosen.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
	return chosen;
}

function attrString(attrs: readonly { name: string; value: string }[]): string {
	return attrs.map(({ name, value }) => ` ${name}="${value}"`).join('');
}

/**
 * Wrap an element snippet so it is structurally valid. The `svg` element is the
 * document root and carries its own attributes; every other element is a
 * self-closing child of a bare `<svg>` root.
 */
function buildSnippet(
	element: CoreElement,
	attrs: readonly { name: string; value: string }[],
): string {
	if (element.name === 'svg') {
		return `<svg${attrString(attrs)}/>`;
	}
	return `<svg><${element.name}${attrString(attrs)}/></svg>`;
}

type ParseResult = { successful: boolean; sexp: string };

/**
 * Parse a snippet with the built grammar and capture the range-free s-expression
 * exactly as `tree-sitter test` would compare it. `successful` reflects the
 * parser's own ERROR/MISSING verdict via the JSON summary; the s-expression is
 * additionally scanned for recovery markers as a defence in depth.
 */
function parseSnippet(snippet: string, workdir: string): ParseResult {
	const file = join(workdir, 'snippet.svg');
	// `tree-sitter test` feeds the corpus input region with its trailing newline,
	// so capture against the identical bytes to keep expected/actual aligned.
	// With `extras: () => []` that trailing `\n` surfaces as a final `(text)`.
	writeFileSync(file, `${snippet}\n`);

	const summaryResult = spawnSync(
		'tree-sitter',
		['parse', '--json-summary', '--quiet', file],
		{ cwd: grammarRoot, encoding: 'utf8' },
	);
	const summaryRaw = summaryResult.stdout;
	const jsonStart = summaryRaw.indexOf('{');
	if (jsonStart === -1) {
		throw new Error(
			`tree-sitter parse did not emit JSON summary for: ${snippet}\n${summaryRaw}${summaryResult.stderr}`,
		);
	}
	const summary = JSON.parse(summaryRaw.slice(jsonStart)) as {
		parse_summaries: { successful: boolean }[];
	};
	const summarySuccessful = summary.parse_summaries.every((s) => s.successful);

	const sexpResult = spawnSync(
		'tree-sitter',
		['parse', '--no-ranges', file],
		{ cwd: grammarRoot, encoding: 'utf8' },
	);
	const sexp = sexpResult.stdout.trimEnd();

	const clean = summarySuccessful && !/\b(ERROR|MISSING)\b/.test(sexp)
		&& !sexp.includes('erroneous');
	return { successful: clean, sexp };
}

type GeneratedCase = { title: string; input: string; sexp: string };

function generateCase(
	element: CoreElement,
	bucketIndex: Map<string, string>,
	coreByName: Map<string, CoreAttribute>,
	workdir: string,
): GeneratedCase {
	const typedAttrs = selectAttributes(element, bucketIndex, coreByName);

	const typedSnippet = buildSnippet(element, typedAttrs);
	const typed = parseSnippet(typedSnippet, workdir);
	if (typedAttrs.length > 0 && typed.successful) {
		return { title: `Element ${element.name} (typed attributes)`, input: typedSnippet, sexp: typed.sexp };
	}

	const bareSnippet = buildSnippet(element, []);
	const bare = parseSnippet(bareSnippet, workdir);
	if (!bare.successful) {
		throw new Error(
			`bare snippet for <${element.name}> does not parse cleanly: ${bareSnippet}\n${bare.sexp}`,
		);
	}
	return { title: `Element ${element.name}`, input: bareSnippet, sexp: bare.sexp };
}

function renderCorpus(cases: readonly GeneratedCase[]): string {
	// Input follows the divider with no intervening blank line: a leading blank
	// would be fed to the parser as a leading `(text)` node (extras are empty).
	const blocks = cases.map(({ title, input, sexp }) =>
		[DIVIDER, title, DIVIDER, input, '', '---', '', sexp].join('\n')
	);
	return `${GENERATED_HEADER}\n\n${blocks.join('\n\n')}\n`;
}

function main(): void {
	const checkMode = argv.includes('--check');

	const core = readJson<CoreCatalog>(corePath);
	const treeSitter = readJson<TreeSitterCatalog>(treeSitterPath);

	const bucketIndex = buildBucketIndex(treeSitter);
	const coreByName = new Map(core.attributes.map((a) => [a.name, a]));

	const elements = [...core.elements].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));

	const workdir = mkdtempSync(join(tmpdir(), 'svg-gen-corpus-'));
	let typedCount = 0;
	try {
		const cases = elements.map((element) => {
			const generated = generateCase(element, bucketIndex, coreByName, workdir);
			if (generated.title.includes('typed')) {
				typedCount += 1;
			}
			return generated;
		});

		const rendered = renderCorpus(cases);

		if (checkMode) {
			let existing: string;
			try {
				existing = readFileSync(outPath, 'utf8');
			} catch {
				existing = '';
			}
			if (existing !== rendered) {
				console.error(
					`generated corpus is stale -> ${displayPath(outPath)}\n`
						+ 'run `bun run gen:corpus` to regenerate.',
				);
				process.exitCode = 1;
				return;
			}
			console.log(`generated corpus up to date -> ${displayPath(outPath)}`);
			console.log(`cases: ${cases.length} (${typedCount} typed, ${cases.length - typedCount} bare)`);
			return;
		}

		writeFileSync(outPath, rendered);
		console.log(`generated corpus written -> ${displayPath(outPath)}`);
		console.log(`source -> ${displayPath(corePath)}`);
		console.log(`cases: ${cases.length} (${typedCount} typed, ${cases.length - typedCount} bare)`);
	} finally {
		rmSync(workdir, { recursive: true, force: true });
	}
}

main();
