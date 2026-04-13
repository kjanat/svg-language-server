import type { ComponentChildren } from "preact";
import { DevReload } from "./DevReload.tsx";

interface Props {
	dev: boolean;
	boot: number;
	title?: string;
	children: ComponentChildren;
}

export function Layout(
	{ dev, boot, title = "SVG Compat", children }: Props,
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
				<main>{children}</main>
				<script type="module" src="/version-picker.mjs"></script>
				<script type="module" src="/table-filter.mjs"></script>
				<DevReload dev={dev} boot={boot} />
			</body>
		</html>
	);
}
