import type { Baseline, BrowserSupport as BrowserSupportData, BrowserVersion } from "../main.ts";
import { browserVersionChipLabel } from "../view.ts";

interface Props {
	support: BrowserSupportData | undefined;
	baselineStatus: Baseline["status"] | undefined;
}

interface BrowserSpec {
	key: keyof BrowserSupportData;
	label: string;
	src: string;
}

const BROWSERS: BrowserSpec[] = [
	{ key: "chrome", label: "Chrome", src: "/browsers/chrome.svg" },
	{ key: "edge", label: "Edge", src: "/browsers/edge.svg" },
	{ key: "firefox", label: "Firefox", src: "/browsers/firefox.svg" },
	{ key: "safari", label: "Safari", src: "/browsers/safari.svg" },
];

const STATUS_SUPPORTED = "/browsers/check.svg";
const STATUS_MISSING = "/browsers/cross.svg";

/** Classifies a browser version for visual styling of its chip. */
function chipStateClass(version: BrowserVersion | undefined): string {
	if (version === undefined) return "chip-missing";
	if (version.supported === false) return "chip-unsupported";
	if (version.version_removed !== undefined) return "chip-removed";
	if (version.partial_implementation) return "chip-partial";
	if (version.flags !== undefined) return "chip-flagged";
	if (version.prefix !== undefined) return "chip-prefixed";
	return "";
}

/**
 * Builds a rich hover title that surfaces every upstream signal we
 * preserved in the pipeline. The chip itself stays visually terse;
 * the title is where the full story lives.
 */
function chipTitle(label: string, version: BrowserVersion | undefined): string {
	if (version === undefined) return `${label} — no data`;
	if (version.supported === false) return `${label} — not supported`;

	const parts: string[] = [];
	if (version.version_added !== undefined) {
		const glyph = version.version_qualifier === "before"
			? "≤"
			: version.version_qualifier === "after"
			? "≥"
			: version.version_qualifier === "approximately"
			? "~"
			: "";
		parts.push(`since ${glyph}${version.version_added}`);
	} else if (version.raw_value_added === true) {
		parts.push("supported (version unknown)");
	} else if (version.raw_value_added === null) {
		parts.push("no version data");
	}

	if (version.version_removed !== undefined) {
		const glyph = version.version_removed_qualifier === "before"
			? "≤"
			: version.version_removed_qualifier === "after"
			? "≥"
			: version.version_removed_qualifier === "approximately"
			? "~"
			: "";
		parts.push(`removed in ${glyph}${version.version_removed}`);
	}
	if (version.partial_implementation) parts.push("partial implementation");
	if (version.prefix !== undefined) parts.push(`prefix ${version.prefix}`);
	if (version.alternative_name !== undefined) {
		parts.push(`alternative name ${version.alternative_name}`);
	}
	if (version.flags !== undefined && version.flags.length > 0) {
		const flagNames = version.flags.map((flag) => flag.name).join(", ");
		parts.push(`behind flag${version.flags.length > 1 ? "s" : ""} ${flagNames}`);
	}
	if (version.notes !== undefined && version.notes.length > 0) {
		parts.push(version.notes.join(" · "));
	}

	return parts.length > 0 ? `${label} — ${parts.join("; ")}` : label;
}

export function BrowserSupport({ support, baselineStatus }: Props) {
	return (
		<ul class="browser-chips" aria-label="Minimum browser versions">
			{BROWSERS.map(({ key, label, src }) => {
				const version = support?.[key];
				const stateClass = chipStateClass(version);
				const classes = `chip chip-${key}${stateClass ? ` ${stateClass}` : ""}`;
				const hasData = version !== undefined && version.supported !== false;
				const statusClass = hasData ? "chip-status--supported" : "chip-status--missing";
				const statusToneClass = hasData && baselineStatus === "newly"
					? " chip-status--newly"
					: "";
				const statusMask = hasData ? STATUS_SUPPORTED : STATUS_MISSING;
				const chipText = version !== undefined ? browserVersionChipLabel(version) : "—";
				return (
					<li class={classes} title={chipTitle(label, version)}>
						<span class="chip-badge">
							<img class="chip-logo" src={src} alt="" width="18" height="18" />
							<span
								class={`chip-status ${statusClass}${statusToneClass}`}
								style={`--chip-status-mask: url('${statusMask}')`}
								aria-hidden="true"
							/>
						</span>
						<span class="chip-version">{chipText}</span>
					</li>
				);
			})}
		</ul>
	);
}
