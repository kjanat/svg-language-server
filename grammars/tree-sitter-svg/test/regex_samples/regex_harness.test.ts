/// <reference types="bun-types" />

import manifestJson from "./manifest.json";
import { beforeAll, describe, expect, test } from "bun:test";

import {
	runHarnessDetailed,
	strictRootOnlyCandidate,
	type CandidateResultDetailed,
} from "./regex_harness";

type JsonObject = Record<string, unknown>;

type ManifestRow = {
	file: string;
	expectedSvg: boolean;
};

function isObject(value: unknown): value is JsonObject {
	return typeof value === "object" && value !== null;
}

function parseManifestRow(value: unknown): ManifestRow {
	if (!isObject(value)) {
		throw new Error("manifest row must be object");
	}

	const file = value.file;
	const expectedSvg = value.expected_svg;

	if (typeof file !== "string") {
		throw new Error("manifest row file must be string");
	}

	if (typeof expectedSvg !== "boolean") {
		throw new Error(`manifest row expected_svg must be boolean for ${file}`);
	}

	return {
		file,
		expectedSvg,
	};
}

function loadManifestRows(): ManifestRow[] {
	const rawManifest: unknown = manifestJson;
	if (!Array.isArray(rawManifest)) {
		throw new Error("manifest.json root must be an array");
	}

	return rawManifest.map(parseManifestRow);
}

const manifestRows = loadManifestRows();
const sampleFiles = manifestRows.map(row => row.file);
const sampleCases = manifestRows.map(row => [row.file, row.expectedSvg] as const);
const isCi = Boolean(process.env.CI);

let results: CandidateResultDetailed[] = [];

function findCandidate(name: string): CandidateResultDetailed {
	const candidate = results.find(entry => entry.candidate.name === name);
	if (candidate === undefined) {
		throw new Error(`missing candidate ${name}`);
	}

	return candidate;
}

function getSample(candidateName: string, file: string) {
	const candidate = findCandidate(candidateName);
	const sample = candidate.samples.find(entry => entry.file === file);
	if (sample === undefined) {
		throw new Error(`missing sample ${file} for candidate ${candidateName}`);
	}

	return sample;
}

describe("regex harness", () => {
	beforeAll(async () => {
		results = await runHarnessDetailed([strictRootOnlyCandidate]);
	});

	test("loads current and experimental candidates", () => {
		expect(findCandidate("current").candidate.name).toBe("current");
		expect(findCandidate("strict_root_only").candidate.name).toBe("strict_root_only");
	});

	test("current candidate has no fixture mismatches", () => {
		const current = findCandidate("current");
		expect(current.falsePositivesEither.length).toBe(0);
		expect(current.falseNegativesEither.length).toBe(0);
	});

	describe("current candidate (tree-sitter.json)", () => {
		test("covers all manifest rows", () => {
			const current = findCandidate("current");
			expect(current.samples.length).toBe(manifestRows.length);

			for (const file of sampleFiles) {
				expect(current.samples.some(sample => sample.file === file)).toBe(true);
			}
		});

		test.each(sampleCases)("current prediction: %s", (file, expected) => {
			const sample = getSample("current", file);
			expect(sample.predictedSvg).toBe(expected);
		});
	});

	describe.skipIf(isCi)("experimental candidate (strict_root_only)", () => {
		test("covers all manifest rows", () => {
			const strict = findCandidate("strict_root_only");
			expect(strict.samples.length).toBe(manifestRows.length);

			for (const file of sampleFiles) {
				expect(strict.samples.some(sample => sample.file === file)).toBe(true);
			}
		});

		test.each(sampleCases)("strict prediction: %s", (file, expected) => {
			const sample = getSample("strict_root_only", file);
			expect(sample.predictedSvg).toBe(expected);
		});
	});
});
