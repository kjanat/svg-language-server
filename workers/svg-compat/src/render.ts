import { escape } from "@std/html";

import type { AttributeEntry, Baseline, BrowserSupport, CompatEntry, SvgCompatOutput } from "./main.ts";

interface NamedCompatEntry extends CompatEntry {
	name: string;
}

interface NamedAttributeEntry extends AttributeEntry {
	name: string;
}

export function buildReloadScript(dev: boolean, boot: number): string {
	if (!dev) return "";
	return `<script>((b)=>{setInterval(()=>fetch("/__reload").then(r=>r.text()).then(t=>{if(t!==b){b=t;location.reload()}}),500)})(${
		JSON.stringify(String(boot))
	})</script>`;
}

function renderBaselineBadge(baseline: Baseline | undefined): string {
	if (!baseline) return `<span class="muted">-</span>`;
	const variant = baseline.status === "widely"
		? "widely"
		: baseline.status === "newly"
		? "newly"
		: "limited";
	const label = baseline.status === "limited"
		? "limited"
		: `${baseline.status} ${baseline.since ?? ""}`.trim();
	return `<span class="badge badge-${variant}">${escape(label)}</span>`;
}

function formatBrowserSupport(browserSupport: BrowserSupport | undefined): string {
	if (!browserSupport) return "-";
	const parts = [
		browserSupport.chrome ? `Chrome ${browserSupport.chrome}` : undefined,
		browserSupport.edge ? `Edge ${browserSupport.edge}` : undefined,
		browserSupport.firefox ? `Firefox ${browserSupport.firefox}` : undefined,
		browserSupport.safari ? `Safari ${browserSupport.safari}` : undefined,
	].filter((value): value is string => value !== undefined);
	return parts.length > 0 ? parts.join(" | ") : "-";
}

function allElements(output: SvgCompatOutput): NamedCompatEntry[] {
	return Object.entries(output.elements)
		.map(([name, entry]) => ({ name, ...entry }));
}

function allAttributes(output: SvgCompatOutput): NamedAttributeEntry[] {
	return Object.entries(output.attributes)
		.map(([name, entry]) => ({ name, ...entry }));
}

function deprecatedEntries(output: SvgCompatOutput): NamedCompatEntry[] {
	return Object.entries(output.elements)
		.filter(([, entry]) => entry.deprecated)
		.map(([name, entry]) => ({ name, ...entry }));
}

function limitedAttributes(output: SvgCompatOutput): NamedAttributeEntry[] {
	return Object.entries(output.attributes)
		.filter(([, entry]) => entry.baseline?.status === "limited")
		.map(([name, entry]) => ({ name, ...entry }));
}

function renderCompatRows(entries: NamedCompatEntry[]): string {
	return entries
		.map((entry) => {
			const mdnCell = entry.mdn_url
				? `<a href="${escape(entry.mdn_url)}">MDN</a>`
				: "-";
			return `<tr><th scope="row"><code>${escape(entry.name)}</code></th><td>${
				renderBaselineBadge(entry.baseline)
			}</td><td>${escape(formatBrowserSupport(entry.browser_support))}</td><td>${mdnCell}</td></tr>`;
		})
		.join("");
}

function renderAttributeRows(entries: NamedAttributeEntry[]): string {
	return entries
		.map((entry) => {
			const mdnCell = entry.mdn_url
				? `<a href="${escape(entry.mdn_url)}">MDN</a>`
				: "-";
			const elements = entry.elements.length === 1 && entry.elements[0] === "*"
				? "global"
				: entry.elements.join(", ");
			return `<tr><th scope="row"><code>${escape(entry.name)}</code></th><td>${escape(elements)}</td><td>${
				renderBaselineBadge(entry.baseline)
			}</td><td>${mdnCell}</td></tr>`;
		})
		.join("");
}

function renderSourceRows(output: SvgCompatOutput): string {
	return Object.values(output.sources)
		.map((source) => {
			return `<tr><th scope="row"><code>${escape(source.package)}</code></th><td>${escape(source.requested)}</td><td>${
				escape(source.resolved)
			}</td><td>${escape(source.mode)}</td><td><a href="${escape(source.source_url)}">source</a></td></tr>`;
		})
		.join("");
}

/**
 * Renders the HTML dashboard page with stats, element/attribute tables, and source info.
 *
 * @param output The processed compat data
 * @param requestUrl Used to build links to JSON/schema endpoints
 * @param reloadScript Dev-only reload helper script
 * @returns Complete HTML string
 */
export function renderHtml(
	output: SvgCompatOutput,
	requestUrl: URL,
	reloadScript = "",
): string {
	const elementCount = Object.keys(output.elements).length;
	const attributeCount = Object.keys(output.attributes).length;
	const deprecatedCount = Object.values(output.elements).filter((entry) => entry.deprecated).length;
	const limitedCount = Object.values(output.attributes).filter((entry) => entry.baseline?.status === "limited")
		.length;
	const jsonUrl = `${requestUrl.origin}/data.json`;
	const schemaUrl = `${requestUrl.origin}/schema.json`;
	const latestHtmlUrl = `${requestUrl.origin}/?source=latest`;
	const latestJsonUrl = `${requestUrl.origin}/data.json?source=latest`;

	return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>SVG Compat</title>
  <link rel="icon" href="/favicon.svg" type="image/svg+xml">
  <link rel="stylesheet" href="/style.css">
</head>
<body>
  <main>
    <div class="hero">
      <p class="muted">SVG compatibility catalog</p>
      <h1>Browser face. Dynamic source knobs.</h1>
      <div class="muted">Generated ${output.generated_at}.<br>
        Ask for <code>/data.json</code>, <code>?format=json</code>, or <code>Accept: application/json</code>.<br>
        Override upstream packages with <code>?source=latest</code> or explicit
        <code>?bcd=</code>
        <span class="ver-wrap">
          <input type="text" class="ver" data-pkg="@mdn/browser-compat-data" data-param="bcd" placeholder="version" autocomplete="off" aria-label="BCD version">
          <ul class="ver-list" hidden></ul>
        </span>
        <code>&amp;wf=</code>
        <span class="ver-wrap">
          <input type="text" class="ver" data-pkg="web-features" data-param="wf" placeholder="version" autocomplete="off" aria-label="Web Features version">
          <ul class="ver-list" hidden></ul>
        </span>.
      </div>
      <p>
        <a class="pill" href="${escape(jsonUrl)}">Open JSON endpoint</a>
        <a class="pill" href="${escape(schemaUrl)}">Open schema</a>
        <a class="pill" href="${escape(latestHtmlUrl)}">Try latest in browser</a>
        <a class="pill" href="${escape(latestJsonUrl)}">Try latest JSON</a>
      </p>
      <div class="stats">
        <div class="stat"><strong>${elementCount}</strong><span class="muted">elements</span></div>
        <div class="stat"><strong>${attributeCount}</strong><span class="muted">attributes</span></div>
        <div class="stat"><strong>${deprecatedCount}</strong><span class="muted">deprecated elements</span></div>
        <div class="stat"><strong>${limitedCount}</strong><span class="muted">limited attributes</span></div>
      </div>
    </div>
    <div class="grid">
      <section>
        <h2>Upstream sources</h2>
        <p class="muted">Effective package versions for this exact response.</p>
        <table>
          <thead>
            <tr><th>Package</th><th>Requested</th><th>Resolved</th><th>Mode</th><th>Link</th></tr>
          </thead>
          <tbody>${renderSourceRows(output)}</tbody>
        </table>
      </section>
      <section>
        <h2>Elements</h2>
        <p class="muted">All elements with baseline and browser floor.</p>
        <table>
          <thead>
            <tr><th>Name</th><th>Baseline</th><th>Support</th><th>Docs</th></tr>
          </thead>
          <tbody>${renderCompatRows(allElements(output))}</tbody>
        </table>
      </section>
      <section>
        <h2>Attributes</h2>
        <p class="muted">All attributes with scope and baseline.</p>
        <table>
          <thead>
            <tr><th>Name</th><th>Elements</th><th>Baseline</th><th>Docs</th></tr>
          </thead>
          <tbody>${renderAttributeRows(allAttributes(output))}</tbody>
        </table>
      </section>
      <section>
        <h2>Deprecated elements</h2>
        <p class="muted">Quick smoke panel for legacy SVG pieces.</p>
        <table>
          <thead>
            <tr><th>Name</th><th>Baseline</th><th>Support</th><th>Docs</th></tr>
          </thead>
          <tbody>${renderCompatRows(deprecatedEntries(output))}</tbody>
        </table>
      </section>
      <section>
        <h2>Limited attributes</h2>
        <p class="muted">Useful for cross-browser pain radar.</p>
        <table>
          <thead>
            <tr><th>Name</th><th>Elements</th><th>Baseline</th><th>Docs</th></tr>
          </thead>
          <tbody>${renderAttributeRows(limitedAttributes(output))}</tbody>
        </table>
      </section>
    </div>
  </main>
  <script type="module" src="/version-picker.mjs"></script>
  ${reloadScript}
</body>
</html>`;
}

export function renderErrorHtml(status: number, message: string, reloadScript = ""): string {
	return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>SVG Compat Error</title>
  <link rel="stylesheet" href="/style.css">
</head>
<body>
  <main class="error-page">
    <article>
      <p><strong>${status}</strong></p>
      <h1>Request failed</h1>
      <p>${escape(message)}</p>
      <p>Ask for <code>application/json</code>, <code>/data.json</code>, or <code>text/html</code>.</p>
    </article>
  </main>
  ${reloadScript}
</body>
</html>`;
}
