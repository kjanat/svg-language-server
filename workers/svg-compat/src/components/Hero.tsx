import type { PageModel } from "../view.ts";
import { StatsGrid } from "./StatsGrid.tsx";
import { VersionCombo } from "./VersionCombo.tsx";

interface Props {
	model: PageModel;
}

export function Hero({ model }: Props) {
	return (
		<header class="hero">
			<div class="hero-main">
				<p class="eyebrow">SVG compatibility catalog</p>
				<h1>Browser face. Dynamic source knobs.</h1>
				<p class="muted hero-description">
					Generated <time>{model.generatedAt}</time>. Ask for <code>/data.json</code>, <code>?format=json</code>, or
					{" "}
					<code>Accept: application/json</code>.
				</p>
				<div class="muted hero-description">
					Override upstream packages with <code>?source=latest</code> or explicit <code>?bcd=</code>
					<VersionCombo
						pkg="@mdn/browser-compat-data"
						param="bcd"
						label="BCD version"
					/>{" "}
					<code>&wf=</code>
					<VersionCombo
						pkg="web-features"
						param="wf"
						label="Web Features version"
					/>.
				</div>
				<nav class="hero-links">
					<a class="pill" href={model.urls.json}>Open JSON endpoint</a>
					<a class="pill" href={model.urls.schema}>Open schema</a>
					<a class="pill" href={model.urls.latestHtml}>Try latest in browser</a>
					<a class="pill" href={model.urls.latestJson}>Try latest JSON</a>
				</nav>
			</div>
			<StatsGrid stats={model.stats} />
		</header>
	);
}
