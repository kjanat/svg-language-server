interface Props {
	mdnUrl?: string;
	specUrls: string[];
	deprecated: boolean;
}

function isW3cSpec(url: string): boolean {
	try {
		const host = new URL(url).hostname.toLowerCase();
		return host === "www.w3.org" || host === "w3.org" || host === "svgwg.org";
	} catch {
		return false;
	}
}

function preferredSpecUrl(specUrls: string[]): string | undefined {
	return specUrls.find((url) => isW3cSpec(url)) ?? specUrls[0];
}

export function DocsLinks({ mdnUrl, specUrls, deprecated }: Props) {
	const specUrl = preferredSpecUrl(specUrls);
	const hasAnyLink = mdnUrl !== undefined || specUrl !== undefined;
	if (!hasAnyLink && !deprecated) return <span class="muted">-</span>;

	return (
		<span class="docs-links">
			{mdnUrl ? <a href={mdnUrl}>MDN</a> : null}
			{specUrl
				? (
					<a href={specUrl}>
						{isW3cSpec(specUrl) ? "W3C" : "Spec"}
					</a>
				)
				: null}
			{deprecated ? <span class="docs-flag docs-flag-deprecated">Deprecated</span> : null}
		</span>
	);
}
