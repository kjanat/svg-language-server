import type { BrowserSupport as BrowserSupportData } from "../main.ts";

interface Props {
	support: BrowserSupportData | undefined;
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

export function BrowserSupport({ support }: Props) {
	return (
		<ul class="browser-chips" aria-label="Minimum browser versions">
			{BROWSERS.map(({ key, label, src }) => {
				const version = support?.[key];
				const hasVersion = version !== undefined;
				return (
					<li
						class={`chip chip-${key}${hasVersion ? "" : " chip-missing"}`}
						title={hasVersion ? `${label} ${version}` : `${label} not supported`}
					>
						<span class="chip-badge">
							<img class="chip-logo" src={src} alt="" width="18" height="18" />
							<img
								class="chip-status"
								src={hasVersion ? STATUS_SUPPORTED : STATUS_MISSING}
								alt=""
								width="14"
								height="18"
							/>
						</span>
						<span class="chip-version">{hasVersion ? version : "—"}</span>
					</li>
				);
			})}
		</ul>
	);
}
