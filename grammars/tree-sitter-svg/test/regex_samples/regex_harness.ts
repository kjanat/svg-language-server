/// <reference types="bun-types" />

import manifestJson from "./manifest.json";
import treeSitterConfigJson from "../../tree-sitter.json";

type JsonObject = Record<string, unknown>;

export type Candidate = {
	name: string;
	firstLineRegex: string;
	contentRegex: string;
};

type ManifestEntry = {
	file: string;
	kind: string;
	expectedSvg: boolean;
};

type SampleInput = {
	file: string;
	kind: string;
	expectedSvg: boolean;
	firstLine: string;
	content: string;
};

export type SampleMatch = {
	file: string;
	kind: string;
	expectedSvg: boolean;
	firstLineMatch: boolean;
	contentMatch: boolean;
	predictedSvg: boolean;
};

type Confusion = {
	tp: number;
	fp: number;
	tn: number;
	fn: number;
};

export type Metrics = Confusion & {
	precision: number;
	recall: number;
	specificity: number;
	accuracy: number;
};

export type CandidateResultDetailed = {
	candidate: Candidate;
	firstLine: Metrics;
	content: Metrics;
	either: Metrics;
	falsePositivesEither: string[];
	falseNegativesEither: string[];
	samples: SampleMatch[];
};

export type CandidateResult = {
	candidate: Candidate;
	firstLine: Metrics;
	content: Metrics;
	either: Metrics;
	falsePositivesEither: string[];
	falseNegativesEither: string[];
};

function isObject(value: unknown): value is JsonObject {
	return typeof value === "object" && value !== null;
}

function parseManifestEntry(value: unknown): ManifestEntry {
	if (!isObject(value)) {
		throw new Error("manifest entry must be object");
	}

	const file = value.file;
	const kind = value.kind;
	const expectedSvg = value.expected_svg;

	if (typeof file !== "string") {
		throw new Error("manifest entry file must be string");
	}

	if (typeof kind !== "string") {
		throw new Error(`manifest entry kind must be string for ${file}`);
	}

	if (typeof expectedSvg !== "boolean") {
		throw new Error(`manifest entry expected_svg must be boolean for ${file}`);
	}

	return {
		file,
		kind,
		expectedSvg,
	};
}

function loadManifestEntries(): ManifestEntry[] {
	const rawManifest: unknown = manifestJson;
	if (!Array.isArray(rawManifest)) {
		throw new Error("manifest.json must contain an array");
	}

	return rawManifest.map(parseManifestEntry);
}

function loadCurrentCandidate(): Candidate {
	const rawConfig: unknown = treeSitterConfigJson;
	if (!isObject(rawConfig)) {
		throw new Error("tree-sitter.json root must be object");
	}

	const grammars = rawConfig.grammars;
	if (!Array.isArray(grammars)) {
		throw new Error("tree-sitter.json must contain a grammars array");
	}

	for (const grammar of grammars) {
		if (!isObject(grammar)) {
			continue;
		}

		if (grammar.name !== "svg") {
			continue;
		}

		const firstLineRegex = grammar["first-line-regex"];
		const contentRegex = grammar["content-regex"];

		if (typeof firstLineRegex !== "string") {
			throw new Error("svg grammar missing first-line-regex");
		}

		if (typeof contentRegex !== "string") {
			throw new Error("svg grammar missing content-regex");
		}

		return {
			name: "current",
			firstLineRegex,
			contentRegex,
		};
	}

	throw new Error("svg grammar not found in tree-sitter.json");
}

function ensureMultiline(flags: string): string {
	return flags.includes("m") ? flags : flags + "m";
}

function compileRegex(pattern: string): RegExp {
	try {
		return new RegExp(pattern, "m");
	} catch {
		const scopedInlineFlags = /^\(\?([ims]+):(.*)\)$/s.exec(pattern);
		if (scopedInlineFlags !== null) {
			const flags = scopedInlineFlags[1];
			const body = scopedInlineFlags[2];
			if (typeof flags === "string" && typeof body === "string") {
				return new RegExp(body, ensureMultiline(flags));
			}
		}

		const leadingInlineFlags = /^\(\?([ims]+)\)(.*)$/s.exec(pattern);
		if (leadingInlineFlags !== null) {
			const flags = leadingInlineFlags[1];
			const body = leadingInlineFlags[2];
			if (typeof flags === "string" && typeof body === "string") {
				return new RegExp(body, ensureMultiline(flags));
			}
		}

		throw new Error(`regex not supported by JS runtime: ${pattern}`);
	}
}

function matchRegex(regex: RegExp, input: string): boolean {
	regex.lastIndex = 0;
	return regex.test(input);
}

function metricsFrom(samples: readonly SampleMatch[], key: "firstLineMatch" | "contentMatch" | "predictedSvg"): Metrics {
	let tp = 0;
	let fp = 0;
	let tn = 0;
	let fn = 0;

	for (const sample of samples) {
		const predictedSvg = sample[key];

		if (predictedSvg && sample.expectedSvg) {
			tp += 1;
			continue;
		}

		if (predictedSvg && !sample.expectedSvg) {
			fp += 1;
			continue;
		}

		if (!predictedSvg && !sample.expectedSvg) {
			tn += 1;
			continue;
		}

		fn += 1;
	}

	const precision = tp + fp === 0 ? 0 : tp / (tp + fp);
	const recall = tp + fn === 0 ? 0 : tp / (tp + fn);
	const specificity = tn + fp === 0 ? 0 : tn / (tn + fp);
	const accuracy = tp + fp + tn + fn === 0 ? 0 : (tp + tn) / (tp + fp + tn + fn);

	return {
		tp,
		fp,
		tn,
		fn,
		precision,
		recall,
		specificity,
		accuracy,
	};
}

async function loadSampleInputs(entries: readonly ManifestEntry[]): Promise<SampleInput[]> {
	return Promise.all(
		entries.map(async (entry): Promise<SampleInput> => {
			const path = `${import.meta.dir}/${entry.file}`;
			const content = await Bun.file(path).text();
			const firstLine = content.split(/\r?\n/, 1)[0] ?? "";

			return {
				file: entry.file,
				kind: entry.kind,
				expectedSvg: entry.expectedSvg,
				firstLine,
				content,
			};
		}),
	);
}

function evaluateCandidate(samples: readonly SampleInput[], candidate: Candidate): CandidateResultDetailed {
	const firstLineRegex = compileRegex(candidate.firstLineRegex);
	const contentRegex = compileRegex(candidate.contentRegex);

	const sampleMatches = samples.map<SampleMatch>(sample => {
		const firstLineMatch = matchRegex(firstLineRegex, sample.firstLine);
		const contentMatch = matchRegex(contentRegex, sample.content);

		return {
			file: sample.file,
			kind: sample.kind,
			expectedSvg: sample.expectedSvg,
			firstLineMatch,
			contentMatch,
			predictedSvg: firstLineMatch || contentMatch,
		};
	});

	return {
		candidate,
		firstLine: metricsFrom(sampleMatches, "firstLineMatch"),
		content: metricsFrom(sampleMatches, "contentMatch"),
		either: metricsFrom(sampleMatches, "predictedSvg"),
		falsePositivesEither: sampleMatches
			.filter(sample => sample.predictedSvg && !sample.expectedSvg)
			.map(sample => sample.file),
		falseNegativesEither: sampleMatches
			.filter(sample => !sample.predictedSvg && sample.expectedSvg)
			.map(sample => sample.file),
		samples: sampleMatches,
	};
}

export async function runHarnessDetailed(candidates: readonly Candidate[]): Promise<CandidateResultDetailed[]> {
	const manifestEntries = loadManifestEntries();
	const sampleInputs = await loadSampleInputs(manifestEntries);
	const current = loadCurrentCandidate();
	const allCandidates = [current, ...candidates];

	return allCandidates.map(candidate => evaluateCandidate(sampleInputs, candidate));
}

export async function runHarness(candidates: readonly Candidate[]): Promise<CandidateResult[]> {
	const detailed = await runHarnessDetailed(candidates);
	return detailed.map(({ candidate, firstLine, content, either, falsePositivesEither, falseNegativesEither }) => ({
		candidate,
		firstLine,
		content,
		either,
		falsePositivesEither,
		falseNegativesEither,
	}));
}

export const strictRootOnlyCandidate: Candidate = {
	name: "strict_root_only",
	firstLineRegex: String.raw`^\s*(?:<!DOCTYPE\s+svg\b|<(?:[A-Za-z_][\w.-]*:)?svg\b)`,
	contentRegex: String.raw`(?is)^\s*(?:<\?xml\b[^>]*\?>\s*)?(?:(?:<!--(?:[^-]|-(?!->))*-->|<\?[^>]*\?>)\s*)*(?:<!DOCTYPE\s+svg\b[^>]*>\s*)?<(?:[A-Za-z_][\w.-]*:)?svg\b`,
};
