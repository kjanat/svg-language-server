import type { PageStats } from "../view.ts";

interface Props {
	stats: PageStats;
}

interface StatCell {
	value: number;
	label: string;
}

export function StatsGrid({ stats }: Props) {
	const cells: StatCell[] = [
		{ value: stats.elements, label: "elements" },
		{ value: stats.attributes, label: "attributes" },
		{ value: stats.deprecated, label: "deprecated" },
		{ value: stats.limited, label: "limited" },
	];
	return (
		<div class="stats">
			{cells.map((cell) => (
				<div class="stat">
					<strong>{cell.value}</strong>
					<span class="stat-label">{cell.label}</span>
				</div>
			))}
		</div>
	);
}
