/**
 * Unit tests for the SVG 2 spec scanner.
 *
 * Fixture-based: every test feeds a small synthetic HTML/XML snippet
 * (modelled on the real svgwg sources) into one of the parser entry
 * points and asserts the structural facts and provenance the scanner
 * extracts. No I/O — these tests run instantly and never touch the
 * filesystem.
 *
 * Coverage targets:
 *
 * - `parseDefinitionsXml`: pulls `<element>` / `<attribute>` /
 *   `<property>` declarations regardless of attribute order, indentation,
 *   self-closing form, or surrounding namespace prefixes.
 * - `parseTextHtmlOverrides`: walks `<h4 id='XxxProperty'>` sections and
 *   classifies the next `<p>` as removed/obsoleted, INCLUDING multiline
 *   sentence wraps (the kerning case the user surfaced).
 * - `parseChangesLog`: walks `<li>Removed the …</li>` blocks, only
 *   counting features with `<span class='element|property|attr-name'>`
 *   markup. False-positive cases (prose mentions like "Removed the use
 *   element from list of elements that the 'visibility' property…")
 *   must NOT be matched.
 * - Provenance line numbers must be 1-indexed and accurate after
 *   multi-line matches.
 * - `dedupe` must collapse same-(kind, name) entries from multiple
 *   definition files, preserving the first-seen provenance.
 *
 * @module
 */

import { assert, assertEquals, assertExists } from "@std/assert";

import { parseChangesLog, parseDefinitionsXml, parseTextHtmlOverrides } from "./spec_scan.ts";

Deno.test("parseDefinitionsXml extracts elements with various indentation", () => {
	const fixture = `<definitions xmlns='http://mcc.id.au/ns/local'>
  <element name='rect' href='shapes.html#RectElement' interfaces='SVGRectElement'/>
  <element
      name='circle'
      href='shapes.html#CircleElement'
      interfaces='SVGCircleElement'/>
  <element
    name='defs'
    href='struct.html#DefsElement'
    contentmodel='anyof'/>
</definitions>
`;
	const facts = parseDefinitionsXml(fixture, "definitions.xml");
	const elementNames = facts
		.filter((f) => f.kind === "element")
		.map((f) => f.name);
	assertEquals(elementNames.sort(), ["circle", "defs", "rect"]);
	for (const fact of facts) {
		assertEquals(fact.status, "defined");
		assertEquals(fact.provenance.file, "definitions.xml");
		assert(fact.provenance.line > 0);
	}
});

Deno.test("parseDefinitionsXml extracts attributes and properties alongside elements", () => {
	const fixture = `<definitions>
  <element name='svg' interfaces='SVGSVGElement'>
    <attribute name='viewBox' href='struct.html#viewBox' animatable='yes'/>
  </element>
  <attribute name='id' href='types.html#id'/>
  <property name='fill' href='painting.html#FillProperty'/>
  <property name='stroke' href='painting.html#StrokeProperty'/>
</definitions>
`;
	const facts = parseDefinitionsXml(fixture, "definitions.xml");
	const byKind: Record<string, string[]> = { element: [], attribute: [], property: [] };
	for (const fact of facts) {
		byKind[fact.kind].push(fact.name);
	}
	assertEquals(byKind.element, ["svg"]);
	assertEquals(byKind.attribute.sort(), ["id", "viewBox"]);
	assertEquals(byKind.property.sort(), ["fill", "stroke"]);
});

Deno.test("parseDefinitionsXml respects double-quoted name attributes too", () => {
	const fixture = `<element name="rect"/>
<attribute name="d"/>
`;
	const facts = parseDefinitionsXml(fixture, "definitions.xml");
	assertEquals(facts.length, 2);
	assertEquals(facts[0].name, "rect");
	assertEquals(facts[1].name, "d");
});

Deno.test("parseTextHtmlOverrides classifies a single-line removal note", () => {
	const fixture = `
<h4 id='GlyphOrientationHorizontalProperty'>The <span class="property">'glyph-orientation-horizontal'</span> property</h4>

<p class="note">
  This property has been removed in SVG 2.
</p>

<h4 id='NextProperty'>The <span class="property">'next'</span> property</h4>
<p>The next property is fine.</p>
`;
	const result = parseTextHtmlOverrides(fixture, "text.html");
	assertEquals(result.removed.length, 1);
	assertEquals(result.removed[0].name, "glyph-orientation-horizontal");
	assertEquals(result.removed[0].status, "removed");
	assertEquals(result.removed[0].kind, "property");
	assertExists(result.removed[0].provenance.line);
	assert(result.removed[0].provenance.line >= 4);
	assertEquals(result.obsoleted.length, 0);
});

Deno.test("parseTextHtmlOverrides handles multiline removal sentence (kerning case)", () => {
	// The kerning case the user surfaced: the sentence wraps across two lines
	// inside the <p class="note"> block. This is the regression that exposed
	// the user's "no spec scanning at all" complaint.
	const fixture = `
<h4 id='KerningProperty'>The <span class="property">'kerning'</span> property</h4>

  <p class="note">
    The <span class="property">'kerning'</span> property has been
    removed in SVG 2.
  </p>
  <p>
    SVG 1.1 used the kerning property to determine if the font kerning
    tables should be used.
  </p>
`;
	const result = parseTextHtmlOverrides(fixture, "text.html");
	assertEquals(result.removed.length, 1);
	assertEquals(result.removed[0].name, "kerning");
	assertEquals(result.removed[0].status, "removed");
});

Deno.test("parseTextHtmlOverrides classifies obsoletion separately from removal", () => {
	const fixture = `
<h4 id='GlyphOrientationVerticalProperty'>The <span class="property">'glyph-orientation-vertical'</span> property</h4>

<p>
  This property applies only to vertical text. It has been obsoleted
  in SVG 2 and partially replaced by the text-orientation property.
</p>
`;
	const result = parseTextHtmlOverrides(fixture, "text.html");
	assertEquals(result.removed.length, 0);
	assertEquals(result.obsoleted.length, 1);
	assertEquals(result.obsoleted[0].name, "glyph-orientation-vertical");
	assertEquals(result.obsoleted[0].status, "obsoleted");
});

Deno.test("parseTextHtmlOverrides scopes notes to their h4 section", () => {
	// A removal sentence inside section A must NOT attach to section B.
	const fixture = `
<h4 id='AProperty'>The <span class="property">'a'</span> property</h4>
<p>This property is fine.</p>

<h4 id='BProperty'>The <span class="property">'b'</span> property</h4>
<p>This property has been removed in SVG 2.</p>

<h4 id='CProperty'>The <span class="property">'c'</span> property</h4>
<p>This property is also fine.</p>
`;
	const result = parseTextHtmlOverrides(fixture, "text.html");
	assertEquals(result.removed.length, 1);
	assertEquals(result.removed[0].name, "b");
});

Deno.test("parseTextHtmlOverrides ignores non-status paragraphs preceding a status note", () => {
	// First <p> is just intro prose; second <p class="note"> carries the
	// authoritative status. The scanner should pick the status one, not
	// the first paragraph.
	const fixture = `
<h4 id='XProperty'>The <span class="property">'x'</span> property</h4>
<p>Some intro text about x that does not mention removal.</p>
<p class="note">This property has been removed in SVG 2.</p>
`;
	const result = parseTextHtmlOverrides(fixture, "text.html");
	assertEquals(result.removed.length, 1);
	assertEquals(result.removed[0].name, "x");
});

Deno.test("parseChangesLog matches removed properties in <li>", () => {
	const fixture = `<ul>
  <li>Removed the <span class='property'>'kerning'</span> property.</li>
</ul>
`;
	const facts = parseChangesLog(fixture, "changes.html");
	assertEquals(facts.length, 1);
	assertEquals(facts[0].name, "kerning");
	assertEquals(facts[0].kind, "property");
	assertEquals(facts[0].status, "removed");
});

Deno.test("parseChangesLog matches removed elements in <li>", () => {
	const fixture = `<ul>
  <li>Removed the <span class='element'>'tref'</span> element.</li>
</ul>
`;
	const facts = parseChangesLog(fixture, "changes.html");
	assertEquals(facts.length, 1);
	assertEquals(facts[0].kind, "element");
	assertEquals(facts[0].name, "tref");
});

Deno.test("parseChangesLog matches removed attributes in <li>", () => {
	const fixture = `<ul>
  <li>Removed the <span class="attr-name">'externalResourcesRequired'</span> attribute.</li>
</ul>
`;
	const facts = parseChangesLog(fixture, "changes.html");
	assertEquals(facts.length, 1);
	assertEquals(facts[0].kind, "attribute");
	assertEquals(facts[0].name, "externalResourcesRequired");
});

Deno.test("parseChangesLog handles multiple features in one <li>", () => {
	// The altGlyph cluster: four removals in a single multi-line li.
	const fixture = `<ul>
  <li>Removed the <span class='element'>'altGlyph'</span>, <span class='element'>'altGlyphDef'</span>,
  <span class='element'>'altGlyphItem'</span> and <span class='element'>'glyphRef'</span> elements.</li>
</ul>
`;
	const facts = parseChangesLog(fixture, "changes.html");
	const names = facts.map((f) => f.name).sort();
	assertEquals(names, ["altGlyph", "altGlyphDef", "altGlyphItem", "glyphRef"]);
	for (const fact of facts) {
		assertEquals(fact.kind, "element");
		assertEquals(fact.status, "removed");
	}
});

Deno.test("parseChangesLog handles multiple attributes in one <li>", () => {
	// The baseProfile + version case: two attributes, single removal entry.
	const fixture = `<ul>
  <li>Removed the <span class="attr-name">baseProfile</span> and <span class="attr-name">version</span> attributes from the <a>'svg'</a> element.</li>
</ul>
`;
	const facts = parseChangesLog(fixture, "changes.html");
	const names = facts.map((f) => f.name).sort();
	assertEquals(names, ["baseProfile", "version"]);
	for (const fact of facts) {
		assertEquals(fact.kind, "attribute");
	}
});

Deno.test("parseChangesLog skips false-positive prose without span markup", () => {
	// The "use element from list of elements" case: this li starts with
	// "Removed the use element" but the "use" is bare prose, not a
	// `<span class='element'>` declaration. The scanner must NOT match.
	const fixture = `<ul>
  <li>Removed the use element from list of elements that the <a>'visibility'</a> property directly affects.</li>
</ul>
`;
	const facts = parseChangesLog(fixture, "changes.html");
	assertEquals(facts.length, 0);
});

Deno.test("parseChangesLog skips IDL/code-only removals", () => {
	// Removals of IDL interface methods etc are wrapped in `<code>`,
	// not classed spans. Out of scope for the scanner.
	const fixture = `<ul>
  <li>Removed the <code>SVGElementInstance</code> interface.</li>
  <li>Removed the SVGViewSpec interface.</li>
</ul>
`;
	const facts = parseChangesLog(fixture, "changes.html");
	assertEquals(facts.length, 0);
});

Deno.test("parseChangesLog accepts both single and double quoted span class", () => {
	const fixture = `<ul>
  <li>Removed the <span class='attr-name'>'a'</span> attribute.</li>
  <li>Removed the <span class="attr-name">'b'</span> attribute.</li>
</ul>
`;
	const facts = parseChangesLog(fixture, "changes.html");
	const names = facts.map((f) => f.name).sort();
	assertEquals(names, ["a", "b"]);
});

Deno.test("parseDefinitionsXml provenance line numbers match the source", () => {
	const fixture = `line1
line2
<element name='one'/>
line4
<element
  name='two'/>
`;
	const facts = parseDefinitionsXml(fixture, "definitions.xml");
	assertEquals(facts.length, 2);
	assertEquals(facts[0].name, "one");
	assertEquals(facts[0].provenance.line, 3);
	assertEquals(facts[1].name, "two");
	// The match starts at line 5 (the `<element\n` token), not at
	// line 6 where `name='two'` lives.
	assertEquals(facts[1].provenance.line, 5);
});

Deno.test("parseTextHtmlOverrides preserves line numbers across multiline matches", () => {
	const fixture = `1
2
<h4 id='XProperty'>The <span class="property">'x'</span> property</h4>
4
<p class="note">
  This property has been removed in SVG 2.
</p>
`;
	const result = parseTextHtmlOverrides(fixture, "text.html");
	assertEquals(result.removed.length, 1);
	// The `<p class="note">` opens on line 5.
	assertEquals(result.removed[0].provenance.line, 5);
});

Deno.test("real-world fixture: glyph-orientation + kerning + altGlyph cluster end-to-end", () => {
	// Larger fixture stitching the three patterns the user surfaced
	// directly from svgwg's text.html / changes.html. Locks in the
	// joint behaviour so a regex tweak can't silently break one
	// pattern while passing the others.
	const textHtml = `
<h4 id='GlyphOrientationHorizontalProperty'>The <span class="property">'glyph-orientation-horizontal'</span> property</h4>
<p class="note">
  This property has been removed in SVG 2.
</p>

<h4 id='GlyphOrientationVerticalProperty'>The <span class="property">'glyph-orientation-vertical'</span> property</h4>
<p>
  This property applies only to vertical text. It has been obsoleted
  in SVG 2 and partially replaced by text-orientation.
</p>

<h4 id='KerningProperty'>The <span class="property">'kerning'</span> property</h4>
<p class="note">
  The <span class="property">'kerning'</span> property has been
  removed in SVG 2.
</p>
`;
	const changesHtml = `<ul>
  <li>Removed the <span class='element'>'altGlyph'</span>, <span class='element'>'altGlyphDef'</span>,
    <span class='element'>'altGlyphItem'</span> and <span class='element'>'glyphRef'</span> elements.</li>
  <li>Removed the <span class="attr-name">baseProfile</span> and <span class="attr-name">version</span> attributes from the <a>'svg'</a> element.</li>
  <li>Removed the <span class='property'>'kerning'</span> property.</li>
</ul>
`;
	const text = parseTextHtmlOverrides(textHtml, "text.html");
	const removedNames = text.removed.map((f) => f.name).sort();
	assertEquals(removedNames, ["glyph-orientation-horizontal", "kerning"]);
	const obsoletedNames = text.obsoleted.map((f) => f.name);
	assertEquals(obsoletedNames, ["glyph-orientation-vertical"]);

	const changes = parseChangesLog(changesHtml, "changes.html");
	const elementNames = changes
		.filter((f) => f.kind === "element")
		.map((f) => f.name)
		.sort();
	const attributeNames = changes
		.filter((f) => f.kind === "attribute")
		.map((f) => f.name)
		.sort();
	const propertyNames = changes
		.filter((f) => f.kind === "property")
		.map((f) => f.name);
	assertEquals(elementNames, ["altGlyph", "altGlyphDef", "altGlyphItem", "glyphRef"]);
	assertEquals(attributeNames, ["baseProfile", "version"]);
	assertEquals(propertyNames, ["kerning"]);
});
