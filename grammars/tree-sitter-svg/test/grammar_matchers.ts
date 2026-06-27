export type CatalogTreeSitter = {
	schema_version: number;
	sources: unknown;
	attribute_buckets: Record<string, string[]>;
	tokens: Record<string, string[]>;
};

export function sortedUnique(values: Iterable<string>): string[] {
	return [...new Set(values)].sort((left, right) => left.localeCompare(right));
}

export function lettersFromCharClass(classBody: string): string[] {
	if (!classBody.startsWith('[') || !classBody.endsWith(']')) {
		throw new Error(`expected a regex character class, got ${classBody}`);
	}

	return sortedUnique(classBody.slice(1, -1));
}

export function extractRegexLiteral(
	source: string,
	constantName: string,
): { pattern: string; flags: string } {
	const pattern = new RegExp(
		`const ${constantName} = /((?:\\\\.|[^/])+)/([a-z]*)\\s*;`,
		'm',
	);
	const match = source.match(pattern);
	if (!match) {
		throw new Error(`grammar.js is missing const ${constantName} = /.../;`);
	}

	return {
		pattern: match[1].replace(/\\(.)/g, '$1'),
		flags: match[2],
	};
}

export function extractInlineRegexLiteral(
	source: string,
	label: string,
): { pattern: string; flags: string } {
	const pattern = new RegExp(`${label}:\\s*_\\s*=>\\s*token\\(/((?:\\\\.|[^/])+)/([a-z]*)\\)`);
	const match = source.match(pattern);
	if (!match) {
		throw new Error(`grammar.js is missing ${label}: _ => token(/.../)`);
	}

	return {
		pattern: match[1].replace(/\\(.)/g, '$1'),
		flags: match[2],
	};
}

export function compileRegexLiteral(literal: { pattern: string; flags: string }): RegExp {
	return new RegExp(literal.pattern, literal.flags);
}

export function setDifference(left: string[], right: string[]): string[] {
	const rightSet = new Set(right);
	return left.filter(value => !rightSet.has(value));
}

export function formatSetMismatch(label: string, actual: string[], expected: string[]): string {
	const missing = setDifference(expected, actual);
	const extra = setDifference(actual, expected);
	const parts: string[] = [label];
	if (missing.length) {
		parts.push(`missing: ${missing.join(', ')}`);
	}
	if (extra.length) {
		parts.push(`extra: ${extra.join(', ')}`);
	}
	return parts.join(' — ');
}

export function expectExactSetMatch(actual: string[], expected: string[], label: string): void {
	const normalizedActual = sortedUnique(actual);
	const normalizedExpected = sortedUnique(expected);
	if (normalizedActual.join('\0') !== normalizedExpected.join('\0')) {
		throw new Error(formatSetMismatch(label, normalizedActual, normalizedExpected));
	}
}

export function expectRegexAcceptsOnly(
	regex: RegExp,
	accepted: string[],
	rejected: string[],
	label: string,
): void {
	for (const sample of accepted) {
		if (!regex.test(sample)) {
			throw new Error(`${label} rejected expected match ${JSON.stringify(sample)}`);
		}
	}

	for (const sample of rejected) {
		if (regex.test(sample)) {
			throw new Error(`${label} accepted unexpected match ${JSON.stringify(sample)}`);
		}
	}
}

export function intersection(left: string[], right: string[]): string[] {
	const rightSet = new Set(right);
	return sortedUnique(left.filter(value => rightSet.has(value)));
}

export function unionBucketAttributes(
	catalog: CatalogTreeSitter,
	keys: Iterable<string>,
): string[] {
	const names: string[] = [];
	for (const key of keys) {
		const bucket = catalog.attribute_buckets[key];
		if (!bucket) {
			throw new Error(`catalog attribute_buckets missing key ${key}`);
		}
		names.push(...bucket);
	}
	return sortedUnique(names);
}

export function bucketAttributeOverlaps(
	catalog: CatalogTreeSitter,
): Array<{ left: string; right: string; names: string[] }> {
	const keys = Object.keys(catalog.attribute_buckets).sort();
	const overlaps: Array<{ left: string; right: string; names: string[] }> = [];
	for (let leftIndex = 0; leftIndex < keys.length; leftIndex += 1) {
		for (let rightIndex = leftIndex + 1; rightIndex < keys.length; rightIndex += 1) {
			const leftKey = keys[leftIndex];
			const rightKey = keys[rightIndex];
			const shared = intersection(
				catalog.attribute_buckets[leftKey],
				catalog.attribute_buckets[rightKey],
			);
			if (shared.length) {
				overlaps.push({ left: leftKey, right: rightKey, names: shared });
			}
		}
	}
	return overlaps;
}

export function expectUpperLowerPair(letters: string[], label: string): void {
	const normalized = sortedUnique(letters);
	if (normalized.length !== 2) {
		throw new Error(`${label} expected exactly two command letters, got ${normalized.join(', ')}`);
	}
	const [left, right] = normalized;
	if (left === right || left.toLowerCase() !== right.toLowerCase()) {
		throw new Error(`${label} expected matching upper/lower pair, got ${normalized.join(', ')}`);
	}
}

export function alphabetExcluding(letters: Iterable<string>): string[] {
	const excluded = new Set(letters);
	const rejected: string[] = [];
	for (let code = 65; code <= 90; code += 1) {
		const upper = String.fromCharCode(code);
		const lower = upper.toLowerCase();
		if (!excluded.has(upper)) {
			rejected.push(upper);
		}
		if (!excluded.has(lower)) {
			rejected.push(lower);
		}
	}
	rejected.push('0', '1', '.', '-', '+', ' ');
	return rejected;
}