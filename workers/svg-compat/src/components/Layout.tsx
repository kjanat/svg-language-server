import type { ComponentChildren } from "preact";
import { DevReload } from "./DevReload.tsx";

interface Props {
	dev: boolean;
	boot: number;
	title?: string;
	/**
	 * When true, skips the default `<main>` wrapper. The caller is
	 * responsible for rendering its own landmark element — used by the
	 * error page, which needs `<main class="error-page">` styles to
	 * override the dashboard `main` layout without nesting landmarks.
	 */
	bare?: boolean;
	/**
	 * Inline style applied to the `<main>` element. Used to pass
	 * computed CSS custom properties (e.g. per-browser chip column
	 * widths derived from the dataset) down to the cascade.
	 */
	mainStyle?: string;
	children: ComponentChildren;
}

/**
 * Document shell. Does not load any interactive scripts itself —
 * each interactive component (TableSection, VersionCombo) emits its
 * own `<script type="module" src>` so script loading is a property
 * of the component tree, not of the layout. Module scripts dedupe
 * by URL so multi-instance rendering is free.
 */
export function Layout(
	{ dev, boot, title = "SVG Compat", bare = false, mainStyle, children }: Props,
) {
	return (
		<html lang="en">
			<head>
				<meta charset="utf-8" />
				<meta name="viewport" content="width=device-width, initial-scale=1" />
				<title>{title}</title>
				<link rel="icon" href="/favicon.svg" type="image/svg+xml" />
				<link rel="stylesheet" href="/style.css" />
			</head>
			<body>
				{bare ? children : <main style={mainStyle}>{children}</main>}
				<DevReload dev={dev} boot={boot} />
			</body>
		</html>
	);
}
