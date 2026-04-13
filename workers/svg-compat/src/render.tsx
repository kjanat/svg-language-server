/**
 * HTML dashboard entry. Composes Preact components into a complete
 * document string via preact-render-to-string. No template literals,
 * no manual escaping — Preact handles both.
 *
 * @module
 */

import { render } from "preact-render-to-string";

import { AttributesTable } from "./components/AttributesTable.tsx";
import { ElementsTable } from "./components/ElementsTable.tsx";
import { ErrorPage } from "./components/ErrorPage.tsx";
import { Hero } from "./components/Hero.tsx";
import { Layout } from "./components/Layout.tsx";
import { TableSection } from "./components/TableSection.tsx";
import { UpstreamSources } from "./components/UpstreamSources.tsx";
import type { SvgCompatOutput } from "./main.ts";
import { BROWSER_KEYS, type BrowserMaxChars, buildPageModel } from "./view.ts";

/**
 * Builds the inline `<main style>` string carrying per-browser chip
 * column widths as CSS custom properties. Derived from the current
 * dataset so adding a new 3-digit Chrome version automatically widens
 * the Chrome column — no stylesheet edit required.
 *
 * Also emits `--chip-chars-max`, used by the container query in the
 * stylesheet: when the chip row wraps (narrow container / mobile) all
 * chips snap to the global max width so the wrapped rows form a tidy
 * grid instead of a ragged one.
 */
function buildChipColumnStyle(maxChars: BrowserMaxChars): string {
	const perBrowser = BROWSER_KEYS.map((key) => `--chip-chars-${key}:${maxChars[key]}`);
	const globalMax = Math.max(...BROWSER_KEYS.map((key) => maxChars[key]));
	return [...perBrowser, `--chip-chars-max:${globalMax}`].join(";");
}

/**
 * Renders the main SVG-compat dashboard as a complete HTML document.
 *
 * @param output Processed compat data
 * @param requestUrl Used to build JSON/schema/latest links
 * @param dev Whether DEV mode is active (emits reload poller)
 * @param boot BOOT timestamp used by the dev reload poller
 */
export function renderHtml(
	output: SvgCompatOutput,
	requestUrl: URL,
	dev = false,
	boot = 0,
): string {
	const model = buildPageModel(output, requestUrl);
	const body = render(
		<Layout dev={dev} boot={boot} mainStyle={buildChipColumnStyle(model.browserMaxChars)}>
			<Hero model={model} />
			<UpstreamSources sources={model.sources} />
			<TableSection
				id="elements"
				title="Elements"
				description="All elements with baseline and browser floor."
				total={model.elements.length}
				placeholder="Filter elements…"
			>
				<ElementsTable rows={model.elements} />
			</TableSection>
			<TableSection
				id="attributes"
				title="Attributes"
				description="All attributes with scope and baseline."
				total={model.attributes.length}
				placeholder="Filter attributes…"
			>
				<AttributesTable rows={model.attributes} />
			</TableSection>
			<TableSection
				id="deprecated"
				title="Deprecated elements"
				description="Quick smoke panel for legacy SVG pieces."
				total={model.deprecatedElements.length}
				placeholder="Filter deprecated…"
			>
				<ElementsTable rows={model.deprecatedElements} />
			</TableSection>
			<TableSection
				id="limited-attributes"
				title="Limited attributes"
				description="Useful for cross-browser pain radar."
				total={model.limitedAttributes.length}
				placeholder="Filter limited…"
			>
				<AttributesTable rows={model.limitedAttributes} />
			</TableSection>
		</Layout>,
	);
	return `<!doctype html>${body}`;
}

/** Renders the standalone error page. */
export function renderErrorHtml(
	status: number,
	message: string,
	dev = false,
	boot = 0,
): string {
	const body = render(
		<Layout dev={dev} boot={boot} bare title="SVG Compat Error">
			<ErrorPage status={status} message={message} />
		</Layout>,
	);
	return `<!doctype html>${body}`;
}
